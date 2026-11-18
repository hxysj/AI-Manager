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

// 使用真实 v3 账号表形状，验证升级时既有配置和历史关联保持不变。
fn account_schema_three(c: &Connection) {
    operation_schema_four(c);
    c.execute_batch("DROP INDEX service_config_google_subject;
        CREATE TABLE service_provider_accounts (
          id TEXT PRIMARY KEY NOT NULL,provider_id TEXT NOT NULL UNIQUE REFERENCES service_provider_configs(id),
          external_account_id TEXT NOT NULL,email TEXT NOT NULL DEFAULT '',
          access_ciphertext TEXT NOT NULL DEFAULT '',refresh_ciphertext TEXT NOT NULL DEFAULT '',id_ciphertext TEXT NOT NULL DEFAULT '',
          expires_at INTEGER,token_generation INTEGER NOT NULL DEFAULT 0,refresh_status TEXT NOT NULL DEFAULT '',
          quota_json TEXT NOT NULL DEFAULT '{}',revision INTEGER NOT NULL DEFAULT 1,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL
        ) STRICT;
        CREATE TABLE service_google_accounts (
          provider_id TEXT PRIMARY KEY NOT NULL REFERENCES service_provider_configs(id),
          external_subject TEXT NOT NULL,email TEXT NOT NULL DEFAULT '',oauth_ciphertext TEXT NOT NULL,
          local_key_ciphertext TEXT NOT NULL,expires_at INTEGER,gateway_port INTEGER NOT NULL,
          metadata_json TEXT NOT NULL DEFAULT '{}',token_generation INTEGER NOT NULL DEFAULT 1,
          revision INTEGER NOT NULL DEFAULT 1,created_at INTEGER NOT NULL,updated_at INTEGER NOT NULL
        ) STRICT;
        CREATE UNIQUE INDEX service_google_subject ON service_google_accounts(external_subject);
        UPDATE app_metadata SET value='3' WHERE key='service_provider:schema_version';").unwrap();
    for column in ACCOUNT_LOCAL_COLUMNS {
        c.execute_batch(&format!("ALTER TABLE service_provider_configs DROP COLUMN {column};")).unwrap();
    }
}

#[test]
fn service_schema_four_merges_official_accounts_without_changing_legacy_or_usage() {
    let f = Fixture::new();
    f.provider();
    let codex = save(&f.paths, &json!({"name":"Codex 官方","kind":"codex_account"})).unwrap();
    let google = save(&f.paths, &json!({"name":"Google 官方","kind":"google_account"})).unwrap();
    provider_store::write_provider_bundle(&f.paths, &[json!({"id":"legacy","cli":"codex","name":"旧服务商",
        "baseUrl":"https://example.test/v1","runtimeConfig":{"mainModel":"original"}})], &[], &[], &Map::new()).unwrap();
    let c = database::open(&f.paths).unwrap();
    let legacy = query(&c, "SELECT * FROM providers", &[]).unwrap();
    let models = query(&c, "SELECT * FROM service_provider_models ORDER BY id", &[]).unwrap();
    c.execute_batch("INSERT INTO service_gateway_requests(id,generation,protocol_in,request_path,state,created_at,finished_at)
        VALUES('historical',1,'responses','/v1/responses','completed',1,2);").unwrap();
    for event in ["INSERT", "UPDATE", "DELETE"] {
        c.execute_batch(&format!("CREATE TRIGGER preserve_legacy_{event} BEFORE {event} ON providers
            BEGIN SELECT RAISE(ABORT,'禁止改写旧表'); END;")).unwrap();
    }
    account_schema_three(&c);
    c.execute("INSERT INTO service_provider_accounts(id,provider_id,external_account_id,email,access_ciphertext,
        refresh_ciphertext,id_ciphertext,expires_at,token_generation,refresh_status,quota_json,created_at,updated_at)
        VALUES('old-credential',?1,'codex-identity','codex@example.test',?2,?3,?4,9999999999000,7,'ready','{\"plan_type\":\"plus\"}',11,22)",
        params![text(&codex,"id"),encrypt_provider_key("old-access").unwrap(),
            encrypt_provider_key("old-refresh").unwrap(),encrypt_provider_key("old-id").unwrap()]).unwrap();
    let tokens = json!({"access_token":"google-access","refresh_token":"google-refresh",
        "client_id":"original-client","client_secret":"original-secret","expires_at":9999999999i64});
    let metadata = json!({"project":"project-one","plan":"pro","quotaGroups":[{"remaining":50}]});
    c.execute("INSERT INTO service_google_accounts(provider_id,external_subject,email,oauth_ciphertext,local_key_ciphertext,
        expires_at,gateway_port,metadata_json,token_generation,created_at,updated_at)
        VALUES(?1,'google-identity','google@example.test',?2,?3,9999999999,34567,?4,5,11,33)",
        params![text(&google,"id"),encrypt_provider_key(&tokens.to_string()).unwrap(),
            encrypt_provider_key("original-local-key").unwrap(),metadata.to_string()]).unwrap();
    initialize(&f.paths).unwrap();
    assert!(!table_exists(&c, "service_provider_accounts").unwrap());
    assert!(!table_exists(&c, "service_google_accounts").unwrap());
    assert_eq!(query(&c,"SELECT value FROM app_metadata WHERE key='service_provider:schema_version'",&[]).unwrap()[0]["value"],"5");
    let account = account(&c, text(&codex,"id")).unwrap();
    assert_eq!(account["accessToken"], "old-access");
    assert_eq!(account["refreshToken"], "old-refresh");
    assert_eq!(account["idToken"], "old-id");
    assert_eq!(account["token_generation"], 7);
    assert_eq!(google_tokens(&c,text(&google,"id")).unwrap(), tokens);
    assert_eq!(google_local_key(&c,text(&google,"id")).unwrap(),"original-local-key");
    let public = state(&f.paths).unwrap();
    assert_eq!(public["accounts"][0]["quota"]["plan_type"],"plus");
    assert_eq!(public["accounts"][0]["refreshStatus"],"ready");
    assert_eq!(public["googleAccounts"][0]["expiresAt"],9999999999000i64);
    assert_eq!(public["googleAccounts"][0]["metadata"],metadata);
    assert!(!public.to_string().contains("old-refresh"));
    assert!(!public.to_string().contains("original-secret"));
    assert_eq!(config(&c,text(&codex,"id")).unwrap()["revision"],1);
    assert_eq!(query(&c,"SELECT * FROM providers",&[]).unwrap(),legacy);
    assert_eq!(query(&c,"SELECT * FROM service_provider_models ORDER BY id",&[]).unwrap(),models);
    assert_eq!(query(&c,"SELECT id FROM service_gateway_requests",&[]).unwrap()[0]["id"],"historical");
    assert!(query(&c,"PRAGMA foreign_key_check",&[]).unwrap().is_empty());
    let mut edited = config(&c,text(&codex,"id")).unwrap();
    edited["name"] = json!("编辑名称");
    edited["credentials_ciphertext"] = json!("界面不得覆盖凭据");
    save(&f.paths,&edited).unwrap();
    initialize(&f.paths).unwrap();
    assert_eq!(super::account(&c,text(&codex,"id")).unwrap()["accessToken"],"old-access");
    save_google_tokens(&f.paths,text(&google,"id"),&tokens,5).unwrap();
    assert!(save_google_tokens(&f.paths,text(&google,"id"),&tokens,5).is_err());
    assert_eq!(config(&c,text(&google,"id")).unwrap()["revision"],1);
    service_provider::switch_backend(&f.paths,"service").unwrap();
    service_provider::switch_backend(&f.paths,"legacy").unwrap();
    assert_eq!(query(&c,"SELECT * FROM providers",&[]).unwrap(),legacy);
}

#[test]
fn service_schema_four_rolls_back_account_upgrade_when_credentials_are_unreadable() {
    let f = Fixture::new();
    let provider = save(&f.paths,&json!({"name":"损坏凭据","kind":"codex_account"})).unwrap();
    let c = database::open(&f.paths).unwrap();
    account_schema_three(&c);
    c.execute("INSERT INTO service_provider_accounts(id,provider_id,external_account_id,access_ciphertext,created_at,updated_at)
        VALUES('unreadable',?1,'identity','not-base64!',1,1)",[text(&provider,"id")]).unwrap();
    let original = query(&c,"SELECT * FROM service_provider_accounts",&[]).unwrap();
    assert!(initialize(&f.paths).unwrap_err().to_string().contains("已保留原账号表"));
    assert_eq!(query(&c,"SELECT * FROM service_provider_accounts",&[]).unwrap(),original);
    assert!(table_exists(&c,"service_google_accounts").unwrap());
    assert_eq!(query(&c,"SELECT value FROM app_metadata WHERE key='service_provider:schema_version'",&[]).unwrap()[0]["value"],"3");
    assert!(!query(&c,"SELECT name FROM pragma_table_info('service_provider_configs')",&[]).unwrap()
        .iter().any(|column|column["name"]=="credentials_ciphertext"));
}

// 还原实际 v4 审计表及明细外键，覆盖已运行新模式的数据库升级。
fn operation_schema_four(c: &Connection) {
    c.execute_batch("DROP TABLE service_provider_migration_items;
        DROP TABLE service_operations;
        CREATE TABLE service_provider_migrations (
          id TEXT PRIMARY KEY NOT NULL,source_schema TEXT NOT NULL,status TEXT NOT NULL,
          created_at INTEGER NOT NULL,completed_at INTEGER,error_message TEXT NOT NULL DEFAULT ''
        ) STRICT;
        CREATE TABLE service_switch_operations (
          id TEXT PRIMARY KEY NOT NULL,from_backend TEXT NOT NULL,to_backend TEXT NOT NULL,
          generation INTEGER NOT NULL,stage TEXT NOT NULL,error_message TEXT NOT NULL DEFAULT '',
          created_at INTEGER NOT NULL,completed_at INTEGER
        ) STRICT;
        CREATE TABLE service_compatibility_tests (
          id TEXT PRIMARY KEY NOT NULL,provider_id TEXT NOT NULL,agent_type TEXT NOT NULL,model_id TEXT NOT NULL,
          provider_revision INTEGER NOT NULL,agent_version TEXT NOT NULL,adapter_version INTEGER NOT NULL DEFAULT 1,
          client_protocol TEXT NOT NULL,upstream_protocol TEXT NOT NULL,capability TEXT NOT NULL,
          status TEXT NOT NULL,evidence_json TEXT NOT NULL,tested_at INTEGER NOT NULL,
          FOREIGN KEY(provider_id,agent_type) REFERENCES service_provider_agent_bindings(provider_id,agent_type),
          FOREIGN KEY(provider_id,model_id) REFERENCES service_provider_models(provider_id,id)
        ) STRICT;
        CREATE TABLE service_provider_migration_items (
          id TEXT PRIMARY KEY NOT NULL,migration_id TEXT NOT NULL REFERENCES service_provider_migrations(id),
          source_table TEXT NOT NULL,source_id TEXT NOT NULL,source_fingerprint TEXT NOT NULL,target_id TEXT,
          status TEXT NOT NULL,reason TEXT NOT NULL DEFAULT '',UNIQUE(migration_id,source_table,source_id)
        ) STRICT;
        CREATE UNIQUE INDEX service_migration_source ON service_provider_migration_items(source_table,source_id)
          WHERE status IN ('copied','unreadable');
        UPDATE app_metadata SET value='4' WHERE key='service_provider:schema_version';").unwrap();
}

#[test]
fn service_schema_five_merges_operation_history_and_preserves_typed_recovery() {
    let f = Fixture::new();
    let provider = f.provider();
    let c = database::open(&f.paths).unwrap();
    let model = query(&c,"SELECT id FROM service_provider_models WHERE provider_id=?1",&[&provider]).unwrap()[0]["id"].clone();
    operation_schema_four(&c);
    c.execute_batch("INSERT INTO service_provider_migrations VALUES
        ('migration-completed','providers-v1','completed',10,20,'保留迁移提示'),
        ('migration-running','providers-v1','running',5,NULL,'');
        INSERT INTO service_switch_operations VALUES
        ('switch-completed','legacy','service',7,'completed','',20,30),
        ('switch-pending','service','legacy',8,'preparing','',40,NULL);
        INSERT INTO service_provider_migration_items VALUES
        ('migration-item','migration-completed','providers','source','original-hash','target','copied','原迁移结果');
        INSERT INTO service_gateway_requests(id,generation,protocol_in,request_path,state,created_at,finished_at)
          VALUES('operation-history-request',7,'responses','/v1/responses','completed',31,32);").unwrap();
    c.execute("INSERT INTO service_compatibility_tests(id,provider_id,agent_type,model_id,provider_revision,
        agent_version,adapter_version,client_protocol,upstream_protocol,capability,status,evidence_json,tested_at)
        VALUES('compatibility-failed',?1,'codex',?2,3,'agent-version',2,'responses','chat','tool_roundtrip_stream',
        'failed','{\"error\":\"保留测试证据\"}',50)",params![provider,model.as_str()]).unwrap();
    let migrations = query(&c,"SELECT * FROM service_provider_migrations ORDER BY id",&[]).unwrap();
    let switches = query(&c,"SELECT * FROM service_switch_operations ORDER BY id",&[]).unwrap();
    let compatibility = query(&c,"SELECT * FROM service_compatibility_tests",&[]).unwrap();
    let items = query(&c,"SELECT * FROM service_provider_migration_items",&[]).unwrap();
    let configs = query(&c,"SELECT * FROM service_provider_configs",&[]).unwrap();
    let requests = query(&c,"SELECT * FROM service_gateway_requests",&[]).unwrap();
    initialize(&f.paths).unwrap();
    for old in ["service_provider_migrations","service_switch_operations","service_compatibility_tests"] {
        assert!(!table_exists(&c,old).unwrap());
    }
    assert_eq!(query(&c,"SELECT id,source_schema,status,created_at,completed_at,error_message
        FROM service_operations WHERE operation_type='migration' ORDER BY id",&[]).unwrap(),migrations);
    assert_eq!(query(&c,"SELECT id,from_backend,to_backend,generation,status AS stage,error_message,created_at,completed_at
        FROM service_operations WHERE operation_type='switch' ORDER BY id",&[]).unwrap(),switches);
    assert_eq!(query(&c,"SELECT id,provider_id,agent_type,model_id,provider_revision,agent_version,adapter_version,
        client_protocol,upstream_protocol,capability,status,evidence_json,completed_at AS tested_at
        FROM service_operations WHERE operation_type='compatibility'",&[]).unwrap(),compatibility);
    assert_eq!(query(&c,"SELECT id,migration_id,source_table,source_id,source_fingerprint,target_id,status,reason
        FROM service_provider_migration_items",&[]).unwrap(),items);
    assert_eq!(query(&c,"SELECT * FROM service_provider_configs",&[]).unwrap(),configs);
    assert_eq!(query(&c,"SELECT * FROM service_gateway_requests",&[]).unwrap(),requests);
    assert!(query(&c,"PRAGMA foreign_key_check",&[]).unwrap().is_empty());
    assert_eq!(query(&c,"SELECT value FROM app_metadata WHERE key='service_provider:schema_version'",&[]).unwrap()[0]["value"],"5");
    let operations = query(&c,"SELECT * FROM service_operations ORDER BY id",&[]).unwrap();
    initialize(&f.paths).unwrap();
    assert_eq!(query(&c,"SELECT * FROM service_operations ORDER BY id",&[]).unwrap(),operations);
    let public = state(&f.paths).unwrap();
    assert_eq!(public["migrations"].as_array().unwrap().len(),2);
    assert_eq!(public["migrations"][0]["copied"],1);
    assert_eq!(public["migrationItems"][0]["reason"],"原迁移结果");
    assert_eq!(public["compatibility"][0]["evidence"]["error"],"保留测试证据");
    assert_eq!(public["compatibility"][0]["testedAt"],50);
    // 迁移明细只能挂在迁移批次下，不能误引用同表中的模式切换记录。
    assert!(c.execute("UPDATE service_provider_migration_items SET migration_id='switch-completed'",[]).is_err());
    service_provider::recover(&f.paths).unwrap();
    assert_eq!(query(&c,"SELECT status FROM service_operations WHERE id='switch-pending'",&[]).unwrap()[0]["status"],"interrupted");
    assert_eq!(query(&c,"SELECT status FROM service_operations WHERE id='migration-running'",&[]).unwrap()[0]["status"],"running");
    assert_eq!(state(&f.paths).unwrap()["compatibility"],public["compatibility"]);
}

#[test]
fn service_schema_five_rolls_back_all_audit_changes_when_history_has_orphan_items() {
    let f = Fixture::new();
    let c = database::open(&f.paths).unwrap();
    operation_schema_four(&c);
    c.execute_batch("INSERT INTO service_provider_migrations VALUES('migration','providers-v1','completed',1,2,'');").unwrap();
    c.pragma_update(None,"foreign_keys","OFF").unwrap();
    c.execute_batch("INSERT INTO service_provider_migration_items
        VALUES('orphan','missing-migration','providers','source','hash',NULL,'skipped','保留原记录');").unwrap();
    c.pragma_update(None,"foreign_keys","ON").unwrap();
    let original = query(&c,"SELECT * FROM service_provider_migration_items",&[]).unwrap();
    assert!(initialize(&f.paths).unwrap_err().to_string().contains("已回滚升级"));
    assert!(!table_exists(&c,"service_operations").unwrap());
    for old in ["service_provider_migrations","service_switch_operations","service_compatibility_tests"] {
        assert!(table_exists(&c,old).unwrap());
    }
    assert_eq!(query(&c,"SELECT * FROM service_provider_migration_items",&[]).unwrap(),original);
    assert_eq!(query(&c,"SELECT value FROM app_metadata WHERE key='service_provider:schema_version'",&[]).unwrap()[0]["value"],"4");
    assert!(query(&c,"PRAGMA foreign_key_list(service_provider_migration_items)",&[]).unwrap()
        .iter().any(|row|row["table"]=="service_provider_migrations"));
}

#[test]
fn service_agent_configuration_switches_one_source_and_restores_all_old_directories() {
    let f = Fixture::new();
    let first = f.provider();
    let second = f.provider();
    let dir = f.root.join("agent-primary");
    let extra = f.root.join("agent-extra");
    for path in [&dir, &extra] {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("config.toml"), "model = \"original\"\n").unwrap();
    }
    let targets = json!([{"id":"codex","installed":true,"configPath":dir}]);
    service_provider::switch_backend(&f.paths, "service").unwrap();
    let c = database::open(&f.paths).unwrap();
    let other_model: String = c.query_row("SELECT id FROM service_provider_models WHERE provider_id=?1",
        [&second], |row| row.get(0)).unwrap();
    let value = |provider: &str| json!({"agentType":"codex","providerId":provider,
        "revision":config(&c,provider).unwrap()["revision"],"defaultModelName":"model",
        "options":{"syncModelList":false}});
    service_provider::configure_agent(&f.paths, &targets, &value(&first)).unwrap();
    service_provider::activate(&f.paths, &targets,
        &json!({"agentType":"codex","providerId":first,"configPath":extra})).unwrap();
    let internal = service_provider::internal_session(&f.paths, &first, "model", "responses").unwrap();
    assert!(service_provider::activate(&f.paths, &targets,
        &json!({"agentType":"codex","providerId":second,"modelId":other_model,"configPath":f.root.join("blocked")}))
        .unwrap_err().to_string().contains("正在使用其他服务商"));
    let mut invalid = value(&second);
    invalid["defaultModelName"] = json!("missing");
    assert!(service_provider::configure_agent(&f.paths, &targets, &invalid).is_err());
    assert!(std::fs::read_to_string(extra.join("config.toml")).unwrap().contains("ai_manager_service"));
    invalid = value(&second);
    invalid["revision"] = json!(0);
    assert!(service_provider::configure_agent(&f.paths, &targets, &invalid).is_err());
    assert!(service_provider::configure_agent(&f.paths, &json!([]), &value(&second)).is_err());
    service_provider::configure_agent(&f.paths, &targets, &value(&second)).unwrap();
    assert_eq!(std::fs::read_to_string(extra.join("config.toml")).unwrap(), "model = \"original\"\n");
    let running = query(&c,"SELECT provider_id FROM service_agent_instances WHERE agent_type='codex'
        AND status='active' AND config_path NOT LIKE 'internal:%'",&[]).unwrap();
    assert_eq!(running.len(),1);
    assert_eq!(running[0]["provider_id"],second);
    assert!(service::authorize(&c,text(&internal,"token")).is_ok());
    service_provider::configure_agent(&f.paths, &targets,
        &json!({"agentType":"codex","providerId":""})).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("config.toml")).unwrap(), "model = \"original\"\n");
}

#[test]
fn service_agent_configuration_keeps_other_agents_authorized() {
    let f = Fixture::new();
    let provider = f.provider();
    service_provider::switch_backend(&f.paths, "service").unwrap();
    let codex = f.root.join("codex");
    let claude = f.root.join("claude");
    let targets = json!([
        {"id":"codex","installed":true,"configPath":codex},
        {"id":"claude","installed":true,"configPath":claude},
        {"id":"claude-desktop","installed":true}
    ]);
    let c = database::open(&f.paths).unwrap();
    let model: String = c.query_row("SELECT id FROM service_provider_models WHERE provider_id=?1",
        [&provider], |row| row.get(0)).unwrap();
    let mut configuration = config(&c, &provider).unwrap();
    configuration["bindings"] = json!([{"agentType":"claude-desktop","enabled":true,"defaultModelName":"model"}]);
    save(&f.paths, &configuration).unwrap();
    let instance = service_provider::activate(&f.paths, &targets,
        &json!({"agentType":"claude","providerId":provider,"modelId":model})).unwrap();
    let claude_before = std::fs::read_to_string(claude.join("settings.json")).unwrap();
    let claude_token = serde_json::from_str::<Value>(&claude_before).unwrap()["env"]["ANTHROPIC_AUTH_TOKEN"].as_str().unwrap().to_string();
    service_provider::configure_agent(&f.paths, &targets,
        &json!({"agentType":"codex","providerId":provider,"revision":config(&c,&provider).unwrap()["revision"],
            "defaultModelName":"model","options":{"syncModelList":false}})).unwrap();
    assert!(service::authorize(&c, &claude_token).is_ok());
    assert_eq!(std::fs::read_to_string(claude.join("settings.json")).unwrap(), claude_before);
    assert_eq!(query(&c,"SELECT status,sync_status FROM service_agent_instances WHERE id=?1",
        &[&text(&instance,"instanceId")]).unwrap()[0],json!({"status":"active","sync_status":"synced"}));
    let codex_before = std::fs::read_to_string(codex.join("config.toml")).unwrap();
    let codex_token = toml_edit::de::from_str::<Value>(&codex_before).unwrap()["model_providers"]["ai_manager_service"]["experimental_bearer_token"].as_str().unwrap().to_string();
    // 在临时目录模拟 Desktop 的接管账本，避免测试触碰本机 Desktop 配置。
    let desktop_path = f.root.join("desktop.json");
    let desktop_original = json!({"deploymentMode":"official","preferences":{"keep":true}}).to_string();
    let desktop_applied = json!({"deploymentMode":"3p","preferences":{"keep":true}}).to_string();
    std::fs::write(&desktop_path, &desktop_applied).unwrap();
    c.execute("INSERT INTO service_agent_instances(id,agent_type,name,config_path,client_protocol,provider_id,model_id,status,sync_status,created_at,updated_at)
        VALUES('desktop','claude-desktop','测试 Desktop',?1,'messages',?2,?3,'active','synced',1,1)",
        params![desktop_path.to_string_lossy(),provider,model]).unwrap();
    c.execute("INSERT INTO service_gateway_tokens(id,token_hash,token_prefix,instance_id,created_at)
        VALUES('desktop-token',?1,'desktop','desktop',1)",[hash("desktop-secret")]).unwrap();
    c.execute("INSERT INTO service_managed_files(id,operation_id,instance_id,file_path,before_exists,before_sha256,before_ciphertext,
        after_sha256,after_ciphertext,managed_fields_json,write_status,created_at,updated_at)
        VALUES('desktop-file','desktop-operation','desktop',?1,1,?2,?3,?4,?5,'{}','written',1,1)",
        params![desktop_path.to_string_lossy(),hash(&desktop_original),encrypt_provider_key(&desktop_original).unwrap(),
            hash(&desktop_applied),encrypt_provider_key(&desktop_applied).unwrap()]).unwrap();
    service_provider::configure_agent(&f.paths, &targets,
        &json!({"agentType":"claude-desktop","providerId":""})).unwrap();
    assert_eq!(std::fs::read_to_string(&desktop_path).unwrap(), desktop_original);
    assert!(service::authorize(&c, "desktop-secret").is_err());
    assert!(service::authorize(&c, &codex_token).is_ok());
    assert!(service::authorize(&c, &claude_token).is_ok());
    service_provider::configure_agent(&f.paths, &targets,
        &json!({"agentType":"claude","providerId":""})).unwrap();
    assert!(service::authorize(&c, &codex_token).is_ok());
    assert_eq!(std::fs::read_to_string(codex.join("config.toml")).unwrap(), codex_before);
    service_provider::configure_agent(&f.paths, &targets,
        &json!({"agentType":"codex","providerId":""})).unwrap();
}

#[test]
fn service_detach_preserves_client_edits_and_preflights_before_revoking_tokens() {
    let f = Fixture::new();
    let provider = f.provider();
    service_provider::switch_backend(&f.paths, "service").unwrap();
    let c = database::open(&f.paths).unwrap();
    let model: String = c.query_row("SELECT id FROM service_provider_models WHERE provider_id=?1",
        [&provider], |row| row.get(0)).unwrap();
    let directory = f.root.join("client");
    std::fs::create_dir_all(&directory).unwrap();
    let original = "model = 'old'\nmodel_provider = 'original'\n[features]\nkeep = true\n";
    std::fs::write(directory.join("config.toml"), original).unwrap();
    let instance = service_provider::activate(&f.paths, &json!([]),
        &json!({"agentType":"codex","providerId":provider,"modelId":model,"configPath":directory})).unwrap();
    let applied = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    let token = toml_edit::de::from_str::<Value>(&applied).unwrap()["model_providers"]["ai_manager_service"]["experimental_bearer_token"].as_str().unwrap().to_string();
    let catalog = std::fs::read(directory.join(".ai_manager_service_models.json")).unwrap();
    std::fs::write(directory.join("config.toml"), "model = [invalid").unwrap();
    assert!(service_provider::detach(&f.paths, text(&instance,"instanceId")).is_err());
    assert!(service::authorize(&c, &token).is_ok());
    assert_eq!(std::fs::read(directory.join(".ai_manager_service_models.json")).unwrap(), catalog);
    // 模拟用户已切换到其他来源，同时客户端写入项目和插件设置。
    let edited = applied.replace("model_provider = \"ai_manager_service\"", "model_provider = 'external'")
        .replace("model = \"model\"", "model = 'user-model'");
    std::fs::write(directory.join("config.toml"), format!("{edited}\n# 客户端新增配置\n[projects.'D:/new-project']\ntrust_level = 'trusted'\n[plugins]\nenabled = true\n")).unwrap();
    service_provider::detach(&f.paths, text(&instance,"instanceId")).unwrap();
    let restored = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    let value = toml_edit::de::from_str::<Value>(&restored).unwrap();
    assert_eq!(value["model_provider"], "external");
    assert_eq!(value["model"], "user-model");
    assert_eq!(value["projects"]["D:/new-project"]["trust_level"], "trusted");
    assert_eq!(value["plugins"]["enabled"], true);
    assert_eq!(value["features"]["keep"], true);
    assert!(restored.contains("# 客户端新增配置"));
    assert!(value.get("model_providers").is_none());
    assert!(!directory.join(".ai_manager_service_models.json").exists());
    assert!(service::authorize(&c, &token).is_err());
    service_provider::switch_backend(&f.paths, "legacy").unwrap();
}

#[test]
fn service_detach_merges_json_and_preserves_deleted_files() {
    let f = Fixture::new();
    let provider = f.provider();
    service_provider::switch_backend(&f.paths, "service").unwrap();
    let c = database::open(&f.paths).unwrap();
    let model: String = c.query_row("SELECT id FROM service_provider_models WHERE provider_id=?1",
        [&provider], |row| row.get(0)).unwrap();
    let directory = f.root.join("claude");
    std::fs::create_dir_all(&directory).unwrap();
    let original = json!({"env":{"USER_VAR":"keep","ANTHROPIC_API_KEY":"original-key"},"permissions":{"allow":["Read"]}});
    std::fs::write(directory.join("settings.json"), original.to_string()).unwrap();
    let value = json!({"agentType":"claude","providerId":provider,"modelId":model,"configPath":directory});
    let instance = service_provider::activate(&f.paths, &json!([]), &value).unwrap();
    let mut edited: Value = serde_json::from_str(&std::fs::read_to_string(directory.join("settings.json")).unwrap()).unwrap();
    edited["env"]["USER_VAR"] = json!("updated");
    edited["env"]["ADDED"] = json!("keep-added");
    edited["model"] = json!("user-model");
    edited["permissions"]["allow"] = json!(["Read", "Edit"]);
    std::fs::write(directory.join("settings.json"), edited.to_string()).unwrap();
    service_provider::detach(&f.paths, text(&instance,"instanceId")).unwrap();
    let restored: Value = serde_json::from_str(&std::fs::read_to_string(directory.join("settings.json")).unwrap()).unwrap();
    assert_eq!(restored, json!({"env":{"USER_VAR":"updated","ADDED":"keep-added","ANTHROPIC_API_KEY":"original-key"},
        "model":"user-model","permissions":{"allow":["Read","Edit"]}}));
    let instance = service_provider::activate(&f.paths, &json!([]), &value).unwrap();
    std::fs::remove_file(directory.join("settings.json")).unwrap();
    service_provider::detach(&f.paths, text(&instance,"instanceId")).unwrap();
    assert!(!directory.join("settings.json").exists());
}

#[test]
fn service_disable_revokes_access_preserves_rules_and_external_changes() {
    let f = Fixture::new();
    let provider = f.provider();
    service_provider::switch_backend(&f.paths, "service").unwrap();
    let dir = f.root.join("disable-client");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("config.toml"), "model = \"before\"\n").unwrap();
    let c = database::open(&f.paths).unwrap();
    let model: String = c.query_row("SELECT id FROM service_provider_models WHERE provider_id=?1",
        [&provider], |row| row.get(0)).unwrap();
    let result = service_provider::activate(&f.paths, &json!([]),
        &json!({"agentType":"codex","providerId":provider,"modelId":model,"configPath":dir})).unwrap();
    c.execute("INSERT INTO service_gateway_requests(id,instance_id,generation,protocol_in,request_path,state,created_at)
        VALUES('disable-inflight',?1,1,'responses','/responses','received',1)",[text(&result,"instanceId")]).unwrap();
    let mut disabled = config(&c,&provider).unwrap();
    disabled["enabled"] = json!(false);
    assert!(save(&f.paths,&disabled).is_err());
    assert_eq!(config(&c,&provider).unwrap()["enabled"],1);
    c.execute("UPDATE service_gateway_requests SET finished_at=2,state='completed'",[]).unwrap();
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(
        service_provider::dispatch(&f.paths,&json!([]),"save",disabled)).unwrap();
    assert_eq!(std::fs::read_to_string(dir.join("config.toml")).unwrap(), "model = \"before\"\n");
    assert_eq!(query(&c,"SELECT count(*) AS count FROM service_agent_instances WHERE status='active'",&[]).unwrap()[0]["count"],0);
    assert_eq!(query(&c,"SELECT count(*) AS count FROM service_gateway_tokens WHERE status='active'",&[]).unwrap()[0]["count"],0);
    assert!(query(&c,"SELECT enabled FROM service_provider_agent_bindings WHERE provider_id=?1",&[&provider]).unwrap().iter().all(|row|row["enabled"]==0));
    assert_eq!(credential(&c,&provider).unwrap()["secret"],"test-secret");
    let mut enabled = config(&c,&provider).unwrap();
    enabled["enabled"] = json!(true);
    enabled["bindings"] = json!([{"agentType":"codex","enabled":true,"defaultModelName":"model","options":{"syncModelList":false}}]);
    save(&f.paths,&enabled).unwrap();
    service_provider::activate(&f.paths, &json!([]),
        &json!({"agentType":"codex","providerId":provider,"configPath":dir})).unwrap();
    std::fs::write(dir.join("config.toml"), "model = \"external-edit\"\n").unwrap();
    let mut disabled = config(&c,&provider).unwrap();
    disabled["enabled"] = json!(false);
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(
        service_provider::dispatch(&f.paths,&json!([]),"save",disabled)).unwrap();
    assert_eq!(config(&c,&provider).unwrap()["enabled"],0);
    assert_eq!(std::fs::read_to_string(dir.join("config.toml")).unwrap(), "model = \"external-edit\"\n");
    assert_eq!(query(&c,"SELECT sync_status FROM service_agent_instances WHERE agent_type='codex'",&[]).unwrap()[0]["sync_status"],"restored");
    assert_eq!(query(&c,"SELECT options_json AS optionsJson FROM service_provider_agent_bindings
        WHERE provider_id=?1 AND agent_type='codex'",&[&provider]).unwrap()[0]["options"]["syncModelList"],false);
}

#[test]
fn service_schema_three_upgrades_old_type_constraint_and_preserves_foreign_keys() {
    let f = Fixture::new();
    let c = database::open(&f.paths).unwrap();
    c.pragma_update(None,"foreign_keys","OFF").unwrap();
    c.execute_batch("DROP TABLE service_provider_configs;").unwrap();
    let old = SCHEMA.split("CREATE TABLE IF NOT EXISTS service_provider_keys").next().unwrap()
        .replace("'api','codex_account','google_account'","'api','codex_account'");
    c.execute_batch(&old).unwrap();
    c.execute_batch("INSERT INTO service_provider_configs(id,name,kind,upstream_protocol,revision,created_at,updated_at)
        VALUES('old-v2','旧版服务商','api','responses',8,1,1);
        INSERT INTO service_provider_models(id,provider_id,upstream_model_id,created_at,updated_at)
        VALUES('old-model','old-v2','model',1,1);
        INSERT INTO service_provider_agent_bindings(provider_id,agent_type,enabled,default_model_id,updated_at)
        VALUES('old-v2','codex',1,'old-model',1);
        UPDATE app_metadata SET value='2' WHERE key='service_provider:schema_version';").unwrap();
    c.pragma_update(None,"foreign_keys","ON").unwrap();
    initialize(&f.paths).unwrap();
    assert_eq!(config(&c,"old-v2").unwrap()["revision"],8);
    assert_eq!(query(&c,"SELECT default_model_id FROM service_provider_agent_bindings",&[]).unwrap()[0]["default_model_id"],"old-model");
    assert!(query(&c,"PRAGMA foreign_key_check",&[]).unwrap().is_empty());
    assert!(save(&f.paths,&json!({"name":"Google","kind":"google_account"})).is_ok());
    assert!(c.execute("UPDATE service_provider_agent_bindings SET default_model_id='missing'",[]).is_err());
}

#[test]
fn service_google_migration_is_read_only_and_uses_separate_encrypted_credentials() {
    let f = Fixture::new();
    let provider = json!({"id":"old-google","cli":"codex","type":"google-account","name":"旧 Google","icon":"google.svg",
        "google":{"subject":"google-subject","email":"google@example.test","port":34567,"proxy":"http://127.0.0.1:7890",
            "models":[{"id":"gemini-test","displayName":"Gemini 测试","maxTokens":100000}]},
        "runtimeConfig":{"mainModel":"gemini-test"}});
    provider_store::write_provider_bundle(&f.paths,&[provider],&[],&[],&Map::new()).unwrap();
    let c = database::open(&f.paths).unwrap();
    let tokens = json!({"access_token":"google-private-access","refresh_token":"google-private-refresh","expires_at":9999999999i64});
    c.execute("UPDATE providers SET oauth_credentials=?1 WHERE id='old-google'",
        [encrypt_provider_key(&tokens.to_string()).unwrap()]).unwrap();
    let before = query(&c,"SELECT * FROM providers",&[]).unwrap();
    for event in ["INSERT","UPDATE","DELETE"] {
        c.execute_batch(&format!("CREATE TRIGGER protect_google_{event} BEFORE {event} ON providers
            BEGIN SELECT RAISE(ABORT,'禁止写旧表'); END;")).unwrap();
    }
    service_provider::switch_backend(&f.paths,"service").unwrap();
    assert_eq!(google_tokens(&c,"old-google").unwrap(),tokens);
    let account = google_provider(&c,"old-google").unwrap();
    assert_eq!(account["google"]["port"],34567);
    assert_eq!(account["google"]["proxy"],"http://127.0.0.1:7890");
    assert_eq!(account["google"]["subject"],"google-subject");
    let public = state(&f.paths).unwrap();
    assert_eq!(public["providers"][0]["kind"],"google_account");
    assert_eq!(public["providers"][0]["hasCredential"],1);
    assert_eq!(public["googleAccounts"][0]["email"],"google@example.test");
    assert!(!public.to_string().contains("google-private"));
    assert!(!query(&c,"SELECT credentials_ciphertext,google_local_key_ciphertext FROM service_provider_configs",&[]).unwrap()[0].to_string().contains("google-private"));
    assert_eq!(public_catalog(&f.paths).unwrap()["providers"][0]["type"],"google-account");
    let mut updated = tokens.clone();
    updated["access_token"] = json!("rotated-google-access");
    save_google_tokens(&f.paths,"old-google",&updated,1).unwrap();
    update_google_metadata(&f.paths,"old-google",&json!({"status":"ready","plan":"test"})).unwrap();
    migrate(&f.paths).unwrap();
    assert_eq!(google_tokens(&c,"old-google").unwrap(),updated);
    assert_eq!(query(&c,"SELECT * FROM providers",&[]).unwrap(),before);
    assert!(service_provider::internal_session(&f.paths,"old-google","gemini-test","responses").is_ok());
    sync_google_models(&f.paths,"old-google",&[json!({"id":"gemini-new","displayName":"Google 新模型","maxTokens":200000})]).unwrap();
    assert_eq!(query(&c,"SELECT enabled FROM service_provider_models WHERE upstream_model_id='gemini-test'",&[]).unwrap()[0]["enabled"],0);
    let model = query(&c,"SELECT id FROM service_provider_models WHERE upstream_model_id='gemini-new'",&[]).unwrap();
    let directory = f.root.join("google-codex");
    service_provider::activate(&f.paths,&json!([]),
        &json!({"agentType":"codex","providerId":"old-google","modelId":model[0]["id"],"configPath":directory})).unwrap();
    let catalog = std::fs::read_to_string(directory.join(".ai_manager_service_models.json")).unwrap();
    assert!(catalog.contains("gemini-new") && !catalog.contains("gemini-test"));
}

#[test]
fn defaults_resolve_draft_model_names_within_the_provider_save_transaction() {
    let f = Fixture::new();
    let result = save(
        &f.paths,
        &json!({
            "name": "新增默认模型",
            "baseUrl": "https://example.test/v1",
            "models": [{"upstreamModelId": "new-model", "enabled": true}],
            "bindings": [{"agentType": "codex", "enabled": true, "defaultModelName": "new-model"}]
        }),
    )
    .unwrap();
    let provider_id = text(&result, "id");
    let c = database::open(&f.paths).unwrap();
    let model_id: String = c
        .query_row(
            "SELECT id FROM service_provider_models WHERE provider_id=?1 AND upstream_model_id='new-model'",
            [provider_id],
            |row| row.get(0),
        )
        .unwrap();
    let default_id: String = c
        .query_row(
            "SELECT default_model_id FROM service_provider_agent_bindings WHERE provider_id=?1 AND agent_type='codex'",
            [provider_id],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(default_id, model_id);

    let mut value = config(&c, provider_id).unwrap();
    value["enabled"] = json!(true);
    value["name"] = json!("不应保存");
    value["bindings"] = json!([
        {"agentType": "codex", "enabled": true, "defaultModelName": "other-provider-model"}
    ]);
    assert!(save(&f.paths, &value).is_err());
    assert_eq!(config(&c, provider_id).unwrap()["name"], "新增默认模型");
    value["models"] = json!([{"upstreamModelId": "new-model", "enabled": false}]);
    value["bindings"][0]["defaultModelName"] = json!("new-model");
    assert!(save(&f.paths, &value).is_err());
    assert_eq!(
        c.query_row(
            "SELECT enabled FROM service_provider_models WHERE id=?1",
            [&model_id],
            |row| row.get::<_, bool>(0),
        )
        .unwrap(),
        true
    );
}

#[test]
fn service_agent_model_rules_are_independent_and_validate_provider_ownership() {
    let f = Fixture::new();
    let provider = f.provider();
    let c = database::open(&f.paths).unwrap();
    let mut value = config(&c, &provider).unwrap();
    value["enabled"] = json!(true);
    value["models"] = json!(["haiku-upstream", "opus-upstream"]);
    value["bindings"] = json!([
        {"agentType":"codex","enabled":true,"defaultModelName":"model","options":{"syncModelList":false}},
        {"agentType":"claude","enabled":true,"defaultModelName":"model","options":{"haikuModel":"haiku-upstream","opusModel":"opus-upstream","sonnetModel":"","subagentModel":"haiku-upstream"}},
        {"agentType":"claude-desktop","enabled":true,"defaultModelName":"model","options":{"modelMappings":{"claude-fable-5":{"model":"opus-upstream","supports1m":true,"labelOverride":"自定义 Fable"}}}}
    ]);
    save(&f.paths, &value).unwrap();
    let saved = state(&f.paths).unwrap();
    let bindings = saved["bindings"].as_array().unwrap();
    assert_eq!(bindings.iter().find(|b| b["agentType"] == "codex").unwrap()["options"]["syncModelList"], false);
    assert_eq!(bindings.iter().find(|b| b["agentType"] == "claude").unwrap()["options"]["haikuModel"], "haiku-upstream");
    let mappings = desktop_model_mappings(&value["bindings"][2]["options"], "model");
    assert_eq!(mappings.len(), 4);
    assert_eq!(mappings[2]["clientModelName"], "claude-fable-5");
    assert!(mappings.iter().all(|m| m["upstreamModelId"] == "opus-upstream" && m["supports1m"] == true));
    assert_eq!(mappings[2]["labelOverride"], "自定义 Fable");
    let mut options = value["bindings"][2]["options"].clone();
    options["modelMappings"]["claude-opus-4-8"] = json!({"supports1m":false});
    assert_eq!(desktop_model_mappings(&options, "model")[1]["supports1m"], false);
    assert_eq!(desktop_model_mappings(&json!({"modelMappings":{}}), "model")[0]["upstreamModelId"], "model");
    assert!(desktop_model_mappings(&json!({}), "model").is_empty());

    let other = f.provider();
    save_model(&c, &other, &json!("other-only")).unwrap();
    for (agent, options) in [
        ("codex", json!({"syncModelList":"true"})),
        ("claude", json!({"haikuModel":"other-only"})),
        ("claude-desktop", json!({"modelMappings":{"claude-sonnet-4-6":{"model":"other-only"}}})),
        ("claude-desktop", json!({"modelMappings":{"unsafe-model":{"model":"model"}}})),
        ("claude-desktop", json!({"modelMappings":{"claude-fable-5":{"supports1m":"true"}}})),
        ("claude", json!({"syncModelList":true})),
    ] {
        assert!(validate_agent_options(&c, &provider, agent, true, &options).is_err());
    }
    // 旧子列表不再生效，即使包含失效或其他服务商的名称也不限制公共目录。
    assert!(validate_agent_options(&c, &provider, "codex", true, &json!({"modelList":["other-only","missing"]})).is_ok());
    assert!(validate_agent_options(&c, &provider, "codex", true, &json!({"modelList":"invalid"})).is_err());
    c.execute("UPDATE service_provider_models SET enabled=0 WHERE provider_id=?1 AND upstream_model_id='opus-upstream'", [&provider]).unwrap();
    assert!(validate_agent_options(&c, &provider, "claude-desktop", true, &value["bindings"][2]["options"]).is_err());
    assert!(validate_agent_options(&c, &provider, "claude-desktop", false, &value["bindings"][2]["options"]).is_ok());
    value["revision"] = config(&c, &provider).unwrap()["revision"].clone();
    value["name"] = json!("无效修改");
    value["models"] = json!(["transaction-only"]);
    value["bindings"][1]["options"]["haikuModel"] = json!("missing");
    assert!(save(&f.paths, &value).is_err());
    assert_ne!(config(&c, &provider).unwrap()["name"], "无效修改");
    assert_eq!(c.query_row("SELECT count(*) FROM service_provider_models WHERE provider_id=?1 AND upstream_model_id='transaction-only'",[&provider],|row|row.get::<_,i64>(0)).unwrap(), 0);
}

#[test]
fn service_codex_sync_switch_controls_catalog_and_claude_writes_each_model() {
    let f = Fixture::new();
    let provider = f.provider();
    service_provider::switch_backend(&f.paths, "service").unwrap();
    let c = database::open(&f.paths).unwrap();
    let directory = f.root.join("codex-model-rules");
    std::fs::create_dir_all(&directory).unwrap();
    let original = "model = \"before\"\nmodel_catalog_json = \"original.json\"\n";
    std::fs::write(directory.join("config.toml"), original).unwrap();
    // 同步开关控制服务商全部可用模型，旧子列表与空数组不再限制目录。
    for (options, expected, main) in [
        (json!({}), true, "model"),
        (json!({"syncModelList":true}), true, "model"),
        (json!({"syncModelList":true,"modelList":["selected"]}), true, "model"),
        (json!({"syncModelList":false,"modelList":["selected"]}), false, "model"),
        (json!({"syncModelList":true,"modelList":[]}), true, "model"),
        (json!({"syncModelList":true,"modelList":["missing"]}), true, "model"),
        (json!({"syncModelList":false,"modelList":["selected"],"modelReasoningEffort":"ultra"}), false, "model"),
        (json!({"syncModelList":false,"modelList":["selected"],"modelReasoningEffort":"ultra"}), false, "gpt-5.6-sol"),
    ] {
        let mut value = config(&c, &provider).unwrap();
        value["enabled"] = json!(true);
        value["models"] = json!(["selected", "other", "gpt-5.6-sol", {"upstreamModelId":"disabled","enabled":false}]);
        value["bindings"] = json!([{"agentType":"codex","enabled":true,"defaultModelName":main,"options":options}]);
        save(&f.paths, &value).unwrap();
        let instance = service_provider::activate(&f.paths, &json!([]),
            &json!({"agentType":"codex","providerId":provider,"configPath":directory})).unwrap();
        let content = std::fs::read_to_string(directory.join("config.toml")).unwrap();
        let catalog_path = directory.join(".ai_manager_service_models.json");
        assert_eq!(content.contains("model_catalog_json"), expected);
        assert_eq!(catalog_path.exists(), expected);
        if options["modelReasoningEffort"] == "ultra" {
            let effort = if main == "gpt-5.6-sol" { "ultra" } else { "low" };
            assert!(content.contains(&format!("model_reasoning_effort = \"{effort}\"")));
        }
        if expected {
            let catalog: Value = serde_json::from_str(&std::fs::read_to_string(&catalog_path).unwrap()).unwrap();
            let names = catalog["models"].as_array().unwrap().iter().map(|model| text(model, "slug")).collect::<Vec<_>>();
            assert!(names.contains(&"selected") && names.contains(&"model"));
            assert!(names.contains(&"other") && names.contains(&"gpt-5.6-sol"));
            assert_eq!(names.len(), 4);
            assert!(!names.contains(&"disabled") && !names.contains(&"missing"));
        }
        service_provider::detach(&f.paths, text(&instance, "instanceId")).unwrap();
        assert_eq!(std::fs::read_to_string(directory.join("config.toml")).unwrap(), original);
        assert!(!catalog_path.exists());
    }
    let directory = f.root.join("claude-model-rules");
    std::fs::create_dir_all(&directory).unwrap();
    let original = "{\"env\":{\"EXISTING\":\"yes\"},\"permissions\":{\"allow\":[]}}";
    std::fs::write(directory.join("settings.json"), original).unwrap();
    let mut value = config(&c, &provider).unwrap();
    value["enabled"] = json!(true);
    value["bindings"] = json!([{"agentType":"claude","enabled":true,"defaultModelName":"model","options":{
        "haikuModel":"selected","sonnetModel":"","opusModel":"other","subagentModel":"selected"
    }}]);
    save(&f.paths, &value).unwrap();
    let instance = service_provider::activate(&f.paths, &json!([]),
        &json!({"agentType":"claude","providerId":provider,"configPath":directory})).unwrap();
    let content: Value = serde_json::from_str(&std::fs::read_to_string(directory.join("settings.json")).unwrap()).unwrap();
    assert_eq!(content["env"]["ANTHROPIC_MODEL"], "model");
    assert_eq!(content["env"]["ANTHROPIC_DEFAULT_HAIKU_MODEL"], "selected");
    assert_eq!(content["env"]["ANTHROPIC_DEFAULT_SONNET_MODEL"], "model");
    assert_eq!(content["env"]["ANTHROPIC_DEFAULT_OPUS_MODEL"], "other");
    assert_eq!(content["env"]["CLAUDE_CODE_SUBAGENT_MODEL"], "selected");
    assert_eq!(content["env"]["EXISTING"], "yes");
    service_provider::detach(&f.paths, text(&instance, "instanceId")).unwrap();
    assert_eq!(std::fs::read_to_string(directory.join("settings.json")).unwrap(), original);
}

#[test]
fn service_official_codex_uses_official_catalog_even_when_legacy_sync_is_enabled() {
    let f = Fixture::new();
    let provider = save(&f.paths, &json!({
        "name":"官方目录","kind":"codex_account","models":["official-model"],
        "bindings":[{"agentType":"codex","enabled":true,"defaultModelName":"official-model","options":{"syncModelList":true}}]
    })).unwrap();
    let provider = text(&provider, "id");
    let c = database::open(&f.paths).unwrap();
    save_account(&c, provider, &json!({"access_token":"access","account_id":"official-id"})).unwrap();
    service_provider::switch_backend(&f.paths, "service").unwrap();
    let directory = f.root.join("official-codex");
    std::fs::create_dir_all(&directory).unwrap();
    let original = "model = \"before\"\nmodel_catalog_json = \"before.json\"\n";
    std::fs::write(directory.join("config.toml"), original).unwrap();
    let instance = service_provider::activate(&f.paths, &json!([]),
        &json!({"agentType":"codex","providerId":provider,"configPath":directory})).unwrap();
    let content = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert!(!content.contains("model_catalog_json"));
    assert!(!directory.join(".ai_manager_service_models.json").exists());
    service_provider::detach(&f.paths, text(&instance, "instanceId")).unwrap();
    assert_eq!(std::fs::read_to_string(directory.join("config.toml")).unwrap(), original);
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
    let runs: i64 = c.query_row("SELECT count(*) FROM service_operations WHERE operation_type='migration'", [], |r| r.get(0)).unwrap();
    service_provider::switch_backend(&f.paths, "service").unwrap();
    assert_eq!(c.query_row("SELECT count(*) FROM service_operations WHERE operation_type='migration'", [], |r| r.get::<_,i64>(0)).unwrap(), runs);
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
    assert_eq!(query(&c, "SELECT status FROM service_operations WHERE operation_type='migration'", &[]).unwrap()[0]["status"], "failed");
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
            "modelList":["m"],"syncModelList":true,
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
    let codex = first["bindings"].as_array().unwrap().iter().find(|binding| binding["agentType"] == "codex").unwrap();
    assert_eq!(codex["options"]["syncModelList"], true);
    assert!(codex["options"].get("modelList").is_none());
    assert_eq!(first["models"][0]["upstreamModelId"], "m");
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
fn service_takeover_restores_original_and_preserves_external_file_changes() {
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
    let original = concat!(
        "model_provider = \"custom\"\nmodel = \"old\"\nopenai_base_url = \"https://old.example.test\"\n",
        "[model_providers.custom] # 旧模式配置 [保留在备份]\nname = \"custom\"\nbase_url = \"https://old.example.test\"\n",
        "[model_providers.custom.http_headers]\nx-old = \"legacy\"\n",
        "[model_providers.codex_local_access]\nbase_url = \"http://127.0.0.1:15721/v1\"\n",
        "[model_providers.ai_manager_service]\nenv_key = \"OLD_ENV\"\n",
        "[model_providers.ai_manager_service.http_headers]\nx-stale = \"stale\"\n",
        "[model_providers.other]\nbase_url = \"https://other.example.test\"\n",
        "[mcp_servers.demo]\ncommand = \"keep-mcp\"\n",
        "[projects.\"D:/project\"]\ntrust_level = \"trusted\"\n",
    );
    std::fs::write(directory.join("config.toml"), original).unwrap();
    let value =
        json!({"agentType":"codex","providerId":provider,"modelId":model,"configPath":directory});
    let instance = service_provider::activate(&f.paths, &json!([]), &value).unwrap();
    let applied = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert!(applied.contains("supports_websockets = false"));
    assert!(applied.contains("model_provider = \"ai_manager_service\""));
    assert!(applied.contains("base_url = \"http://127.0.0.1:15724/v1\""));
    assert!(applied.contains("requires_openai_auth = false"));
    assert_eq!(applied.matches("[model_providers.ai_manager_service]").count(), 1);
    assert!(!applied.contains("model_providers.custom"));
    assert!(!applied.contains("model_providers.codex_local_access"));
    assert!(!applied.contains("old.example.test"));
    assert!(!applied.contains("OLD_ENV"));
    assert!(!applied.contains("x-stale"));
    assert!(applied.contains("[model_providers.other]"));
    assert!(applied.contains("command = \"keep-mcp\""));
    assert!(applied.contains("trust_level = \"trusted\""));
    assert!(!applied.contains("test-secret"));
    std::fs::write(
        directory.join("config.toml"),
        format!("{applied}# 用户修改\n"),
    )
    .unwrap();
    service_provider::switch_backend(&f.paths, "legacy").unwrap();
    assert!(!active(&f.paths).unwrap());
    assert!(std::fs::read_to_string(directory.join("config.toml"))
        .unwrap()
        .contains("# 用户修改"));
    let restored = std::fs::read_to_string(directory.join("config.toml")).unwrap();
    assert_eq!(toml_edit::de::from_str::<Value>(&restored).unwrap(), toml_edit::de::from_str::<Value>(original).unwrap());
    assert!(!directory.join(".ai_manager_service_models.json").exists());
    service_provider::switch_backend(&f.paths, "service").unwrap();
    std::fs::write(directory.join("config.toml"), original).unwrap();
    service_provider::activate(&f.paths, &json!([]), &value).unwrap();
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
fn service_model_pricing_inherits_global_without_overwriting_provider_prices() {
    let pricing = json!({"exchangeRate":7.5,"items":[
        {"id":"global","modelId":" Model ","currency":"CNY",
         "inputCostPerMillion":7.5,"cacheReadCostPerMillion":0.75,"outputCostPerMillion":15.0},
        {"id":"duplicate","modelId":"model","currency":"USD","inputCostPerMillion":99.0}
    ]});
    let model = json!({"upstreamModelId":"MODEL","currency":"USD"});
    let inherited = model_pricing(&model, &pricing);
    assert_eq!(inherited["source"], "global");
    assert_eq!(inherited["currency"], "CNY");
    assert_eq!(inherited["inputPriceMicros"], 7_500_000.0);
    assert_eq!(inherited["cachedInputPriceMicros"], 750_000.0);
    assert_eq!(inherited["outputPriceMicros"], 15_000_000.0);
    assert_eq!(inherited["usagePricingSnapshot"]["model"]["id"], "global");
    assert_eq!(inherited["usagePricingSnapshot"]["exchangeRate"], 7.5);
    assert!(model["inputPriceMicros"].is_null());
    for price in [0, 1_000_000] {
        let custom = model_pricing(&json!({"upstreamModelId":"model","currency":"USD","inputPriceMicros":price}), &pricing);
        assert_eq!(custom["source"], "provider");
        assert_eq!(custom["currency"], "USD");
        assert_eq!(custom["inputPriceMicros"], price);
        assert!(custom["outputPriceMicros"].is_null());
        assert!(custom["usagePricingSnapshot"].is_null());
    }
    assert_eq!(model_pricing(&json!({"upstreamModelId":"unmatched"}), &pricing)["source"], "unknown");
    assert_eq!(model_pricing(&model, &json!({"items":[]}))["source"], "unknown");
}

#[test]
fn service_model_pricing_state_keeps_inherited_values_out_of_saved_models() {
    let f = Fixture::new();
    let provider = f.provider();
    let c = database::open(&f.paths).unwrap();
    c.execute("UPDATE service_provider_models SET input_price_micros=NULL,cached_input_price_micros=NULL,
        output_price_micros=NULL WHERE provider_id=?1", [&provider]).unwrap();
    assert_eq!(state(&f.paths).unwrap()["models"][0]["effectivePricing"]["source"], "unknown");
    crate::core::usage_store::write_pricing(&f.paths, &json!({"exchangeRate":7.5,"items":[{
        "id":"global","modelId":"model","currency":"CNY","inputCostPerMillion":7.5,
        "cacheReadCostPerMillion":0.75,"outputCostPerMillion":15.0
    }]})).unwrap();
    let data = state(&f.paths).unwrap();
    let model = data["models"].as_array().unwrap().iter().find(|model| model["providerId"] == provider).unwrap();
    assert!(model["inputPriceMicros"].is_null());
    assert_eq!(model["effectivePricing"]["source"], "global");
    assert_eq!(model["effectivePricing"]["currency"], "CNY");
    assert_eq!(model["effectivePricing"]["inputPriceMicros"], 7_500_000.0);
    save_model(&c, &provider, model).unwrap();
    let saved = query(&c, "SELECT input_price_micros,cached_input_price_micros,output_price_micros,currency
        FROM service_provider_models WHERE provider_id=?1", &[&provider]).unwrap();
    assert!(saved[0]["input_price_micros"].is_null());
    assert!(saved[0]["cached_input_price_micros"].is_null());
    assert!(saved[0]["output_price_micros"].is_null());
    assert_eq!(saved[0]["currency"], "USD");
}

#[test]
fn service_ledger_uses_global_price_snapshot_and_preserves_unknown_usage() {
    let f = Fixture::new();
    let provider = f.provider();
    let c = database::open(&f.paths).unwrap();
    let model = query(&c, "SELECT id FROM service_provider_models WHERE provider_id=?1", &[&provider]).unwrap();
    let pricing = json!({"exchangeRate":7.5,"items":[{
        "id":"global","modelId":"model","currency":"CNY","inputCostPerMillion":7.5,
        "cacheReadCostPerMillion":0.75,"outputCostPerMillion":15.0
    }]});
    let snapshot = model_pricing(&json!({"upstreamModelId":"model"}), &pricing);
    for id in ["priced", "unknown"] {
        c.execute("INSERT INTO service_gateway_requests(id,generation,protocol_in,request_path,state,created_at)
            VALUES(?1,0,'responses','/responses','routed',1)", [id]).unwrap();
        c.execute("INSERT INTO service_gateway_attempts(id,request_id,attempt_no,provider_id,provider_name,credential_id,
            model_id,upstream_model,protocol_out,state,started_at,price_snapshot_json,currency)
            VALUES(?1,?1,1,?2,'测试','key',?3,'model','responses','started',1,?4,'CNY')",
            params![id,provider,model[0]["id"].as_str().unwrap(),snapshot.to_string()]).unwrap();
    }
    crate::core::usage_store::write_pricing(&f.paths, &json!({"exchangeRate":9.0,"items":[{
        "id":"global","modelId":"model","currency":"USD","inputCostPerMillion":99.0
    }]})).unwrap();
    let record = json!({"serviceRequestId":"priced","attemptId":"priced","ok":true,"statusCode":200,
        "measuredInput":100,"measuredOutput":10,"measuredCache":20});
    service::settle(&f.paths, &record).unwrap();
    service::settle(&f.paths, &json!({"serviceRequestId":"unknown","attemptId":"unknown","ok":true,"statusCode":200})).unwrap();
    let mut repeated = record.clone();
    repeated["measuredInput"] = json!(999);
    service::settle(&f.paths, &repeated).unwrap();
    let rows = query(&c, "SELECT id,estimated_cost_micros,price_snapshot_json AS priceJson,currency
        FROM service_gateway_attempts ORDER BY id", &[]).unwrap();
    assert_eq!(rows[0]["estimated_cost_micros"], 765);
    assert_eq!(rows[0]["currency"], "CNY");
    assert_eq!(rows[0]["price"]["usagePricingSnapshot"]["exchangeRate"], 7.5);
    assert!(rows[1]["estimated_cost_micros"].is_null());
    assert_eq!(usage(&c).unwrap()["attempts"]["attemptCount"], 2);
}

#[test]
fn service_ledger_backfills_only_complete_unpriced_requests_and_locks_the_snapshot() {
    let f = Fixture::new();
    let provider = f.provider();
    let mut c = database::open(&f.paths).unwrap();
    let model: String = c.query_row("SELECT id FROM service_provider_models WHERE provider_id=?1",
        [&provider], |row| row.get(0)).unwrap();
    let empty = json!({"inputPriceMicros":null,"cachedInputPriceMicros":null,
        "outputPriceMicros":null,"currency":"USD","audit":"preserved"});
    for (id, finished, input, cached, cost, snapshot) in [
        ("historical", Some(2), Some(171775), Some(171008), None, empty.clone()),
        ("locked", Some(2), Some(171775), Some(171008), Some(0), empty.clone()),
        ("partial-usage", Some(2), Some(171775), None, None, empty.clone()),
        ("in-flight", None, Some(171775), Some(171008), None, empty.clone()),
        ("invalid-cache", Some(2), Some(1), Some(2), None, empty.clone()),
        ("unknown-model", Some(2), Some(171775), Some(171008), None, empty.clone()),
        ("partial-price", Some(2), Some(171775), Some(171008), None,
            json!({"inputPriceMicros":1_000_000,"currency":"CNY"})),
        ("snapshot", Some(2), Some(100), Some(20), None,
            json!({"inputPriceMicros":1_000_000,"cachedInputPriceMicros":500_000,
                "outputPriceMicros":2_000_000,"currency":"CNY"})),
    ] {
        c.execute("INSERT INTO service_gateway_requests(id,generation,protocol_in,request_path,state,created_at,finished_at)
            VALUES(?1,0,'responses','/responses','completed',1,?2)", params![id,finished]).unwrap();
        c.execute("INSERT INTO service_gateway_attempts(id,request_id,attempt_no,provider_id,provider_name,credential_id,
            model_id,upstream_model,protocol_out,state,started_at,finished_at,input_tokens,output_tokens,
            cached_input_tokens,estimated_cost_micros,price_snapshot_json,currency)
            VALUES(?1,?1,1,?2,'测试','key',?3,?4,'responses','succeeded',1,?5,?6,505,?7,?8,?9,'USD')",
            params![id,provider,model,if id=="unknown-model" {"missing"} else {"model"},finished,input,cached,cost,snapshot.to_string()]).unwrap();
    }
    crate::core::usage_store::write_pricing(&f.paths, &json!({"exchangeRate":7.2,"items":[{
        "id":"global","modelId":"model","currency":"USD","inputCostPerMillion":5.0,
        "cacheReadCostPerMillion":0.5,"outputCostPerMillion":30.0
    }]})).unwrap();
    // 普通状态刷新同时修复明细和汇总，无需修改原 Usage 表或重新导入日志。
    let refreshed = state(&f.paths).unwrap();
    let read = |c: &Connection| query(c, "SELECT id,estimated_cost_micros AS cost,currency,price_snapshot_json AS priceJson
        FROM service_gateway_attempts ORDER BY id", &[]).unwrap();
    let rows = read(&c);
    let historical = rows.iter().find(|row| row["id"] == "historical").unwrap();
    assert_eq!(historical["cost"], 104489);
    assert_eq!(historical["price"]["source"], "global");
    assert_eq!(historical["price"]["audit"], "preserved");
    assert_eq!(historical["price"]["usagePricingSnapshot"]["exchangeRate"], 7.2);
    for id in ["partial-usage", "in-flight", "invalid-cache", "unknown-model", "partial-price"] {
        assert!(rows.iter().find(|row| row["id"] == id).unwrap()["cost"].is_null(), "{id}");
    }
    assert_eq!(rows.iter().find(|row| row["id"] == "locked").unwrap()["cost"], 0);
    let original = rows.iter().find(|row| row["id"] == "snapshot").unwrap();
    assert_eq!(original["cost"], 1100);
    assert_eq!(original["currency"], "CNY");
    assert_eq!(refreshed["usage"]["byProvider"].as_array().unwrap().iter()
        .map(|group| group["pricedCount"].as_i64().unwrap()).sum::<i64>(), 3);
    crate::core::usage_store::write_pricing(&f.paths, &json!({"exchangeRate":9.0,"items":[{
        "id":"global","modelId":"model","currency":"CNY","inputCostPerMillion":99.0,
        "cacheReadCostPerMillion":99.0,"outputCostPerMillion":99.0
    }]})).unwrap();
    backfill_usage_costs(&mut c).unwrap();
    assert_eq!(read(&c), rows);
}

#[test]
fn service_request_cost_preserves_free_and_fractional_prices_without_inventing_usage() {
    let free = json!({"inputPriceMicros":0,"outputPriceMicros":0});
    assert_eq!(estimate_request_cost(Some(100), Some(20), Some(0), &free), Some(0));
    assert_eq!(estimate_request_cost(Some(100), Some(20), None, &free), None);
    assert_eq!(estimate_request_cost(Some(100), Some(20), Some(101), &free), None);
    assert_eq!(estimate_request_cost(Some(-1), Some(20), Some(0), &free), None);
    let fractional = json!({"source":"global","inputPriceMicros":0.5,
        "cachedInputPriceMicros":0.25,"outputPriceMicros":0.5});
    assert_eq!(estimate_request_cost(Some(1_000_000), Some(1_000_000), Some(0), &fractional), Some(1));
    assert_eq!(estimate_request_cost(Some(1), Some(0), Some(0), &fractional), Some(1));
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
            "SELECT status FROM service_operations WHERE operation_type='migration'",
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
        "UPDATE service_provider_configs SET quota_json='{\"plan_type\":\"plus\"}' WHERE id=?1",
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
    assert_eq!(account["token_generation"], 3);
    assert_eq!(state(&f.paths).unwrap()["accounts"][0]["quota"], json!({}));
}

#[test]
fn service_account_refresh_error_is_bound_to_credentials_and_cleared_after_authorization() {
    let f=Fixture::new();
    let provider=save(&f.paths,&json!({"name":"授权状态测试","kind":"codex_account"})).unwrap();
    let provider=text(&provider,"id");
    let c=database::open(&f.paths).unwrap();
    let auth=json!({"account_id":"account-a","access_token":"access-a","expires_at":1});
    save_account(&c,provider,&auth).unwrap();
    let generation=account(&c,provider).unwrap()["token_generation"].as_i64().unwrap();
    c.execute("UPDATE service_provider_configs SET quota_json='{\"plan_type\":\"plus\"}' WHERE id=?1",[provider]).unwrap();
    let error=ManagerError::OpenAiRequest{status_code:401,message:json!({"error":{"code":"token_expired","message":"Your authentication token has expired"}}).to_string()};
    record_account_refresh_error(&c,provider,generation,&error).unwrap();
    let failed=state(&f.paths).unwrap()["accounts"][0].clone();
    assert_eq!(failed["refreshStatus"],"failed");
    assert_eq!(failed["quota"]["refreshError"]["statusCode"],401);
    assert!(text(&failed["quota"]["refreshError"],"message").contains("token_expired"));
    assert_eq!(failed["quota"]["plan_type"],"plus");
    // 重新授权清除错误但保留同一账号的额度，旧请求不能污染新凭据的状态。
    save_account(&c,provider,&auth).unwrap();
    record_account_refresh_error(&c,provider,generation,&error).unwrap();
    let refreshed=state(&f.paths).unwrap()["accounts"][0].clone();
    assert_eq!(refreshed["refreshStatus"],"");
    assert!(refreshed["quota"]["refreshError"].is_null());
    assert_eq!(refreshed["quota"]["plan_type"],"plus");
    // 实际刷新流程在本地发现凭据过期，也必须保存到对应账号，且不发起外部请求。
    let runtime=tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    assert!(runtime.block_on(crate::api::codex_account::refresh_service_quota(&f.paths,provider)).is_err());
    let failed=state(&f.paths).unwrap()["accounts"][0].clone();
    assert_eq!(failed["refreshStatus"],"failed");
    assert!(text(&failed["quota"]["refreshError"],"message").contains("凭据已过期"));
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
    let tool_request = std::sync::Arc::new(gateway_request(&f.paths).unwrap());
    let tool_stream = tool_request.clone();
    assert!(service_provider::switch_backend(&f.paths, "legacy").is_err());
    drop(tool_request);
    assert!(service_provider::switch_backend(&f.paths, "legacy").is_err());
    drop(tool_stream);
    let gate = pause_legacy(&f.paths).unwrap();
    assert!(gateway_request(&f.paths).is_err());
    drop(gate);
    service_provider::switch_backend(&f.paths, "legacy").unwrap();
    assert!(legacy_request(&f.paths).is_ok());
}

#[test]
fn service_client_token_survives_provider_edits_switches_and_recovery_until_detached() {
    let f = Fixture::new();
    let first = f.provider();
    let second = f.provider();
    service_provider::switch_backend(&f.paths,"service").unwrap();
    let c = database::open(&f.paths).unwrap();
    for agent in ["codex","claude"] {
        let directory = f.root.join(agent);
        std::fs::create_dir_all(&directory).unwrap();
        let file = directory.join(if agent=="codex" {"config.toml"} else {"settings.json"});
        let original = if agent=="codex" {"model = 'original'\n# 保留原配置\n"} else {"{\"env\":{\"USER_VAR\":\"original\"}}"};
        std::fs::write(&file, original).unwrap();
        let targets = json!([{"id":agent,"installed":true,"configPath":directory}]);
        let active = service_provider::quick_switch(&f.paths,&targets,&json!({"agentType":agent,"providerId":first})).unwrap();
        let read_token = || {
            let content = std::fs::read_to_string(&file).unwrap();
            let value: Value = if agent=="codex" {toml_edit::de::from_str(&content).unwrap()} else {serde_json::from_str(&content).unwrap()};
            if agent=="codex" {value["model_providers"]["ai_manager_service"]["experimental_bearer_token"].as_str().unwrap().to_string()}
            else {value["env"]["ANTHROPIC_AUTH_TOKEN"].as_str().unwrap().to_string()}
        };
        let token = read_token();
        let token_id = service::authorize(&c,&token).unwrap()["tokenId"].clone();
        let mut config = super::config(&c,&first).unwrap();
        config["name"] = json!("改名仍可用");
        config["apiKey"] = json!("updated-upstream-secret");
        config["baseUrl"] = json!("http://127.0.0.1:2/v1");
        config["models"] = json!(["model","new-model"]);
        save(&f.paths,&config).unwrap();
        assert_eq!(read_token(),token);
        assert_eq!(service::authorize(&c,&token).unwrap()["tokenId"],token_id);
        if agent=="codex" {
            let catalog: Value = serde_json::from_str(&std::fs::read_to_string(directory.join(".ai_manager_service_models.json")).unwrap()).unwrap();
            assert!(catalog["models"].as_array().unwrap().iter().any(|model|model["slug"]=="new-model"));
        }
        service_provider::quick_switch(&f.paths,&targets,&json!({"agentType":agent,"providerId":second})).unwrap();
        assert_eq!(read_token(),token);
        let identity = service::authorize(&c,&token).unwrap();
        assert_eq!(identity["providerId"],second);
        assert_eq!(identity["tokenId"],token_id);
        // 旧版 pending 在重启恢复时自动同步，不能要求重新配置客户端。
        c.execute("UPDATE service_agent_instances SET sync_status='pending' WHERE id=?1",[text(&active,"instanceId")]).unwrap();
        service_provider::recover(&f.paths).unwrap();
        assert_eq!(read_token(),token);
        assert!(service::authorize(&c,&token).is_ok());
        service_provider::configure_agent(&f.paths,&targets,&json!({"agentType":agent,"providerId":second,
            "revision":super::config(&c,&second).unwrap()["revision"],"defaultModelName":"model",
            "options":if agent=="codex" {json!({"syncModelList":false})} else {json!({})}})).unwrap();
        assert_eq!(read_token(),token);
        assert!(service::authorize(&c,&token).is_ok());
        if agent=="codex" {assert!(!directory.join(".ai_manager_service_models.json").exists());}
        service_provider::detach(&f.paths,text(&active,"instanceId")).unwrap();
        assert!(service::authorize(&c,&token).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(),original);
        service_provider::quick_switch(&f.paths,&targets,&json!({"agentType":agent,"providerId":second})).unwrap();
        assert_ne!(read_token(),token);
        assert!(service::authorize(&c,&token).is_err(),"已撤销的令牌不能因再次启用而复活");
        service_provider::detach(&f.paths,text(&active,"instanceId")).unwrap();
    }
}

#[test]
fn service_sync_failure_keeps_client_token_and_previous_file_snapshots() {
    let f=Fixture::new();
    let first=f.provider();
    let second=f.provider();
    service_provider::switch_backend(&f.paths,"service").unwrap();
    let directory=f.root.join("client");
    let targets=json!([{"id":"codex","configPath":directory}]);
    let instance=service_provider::quick_switch(&f.paths,&targets,&json!({"agentType":"codex","providerId":first})).unwrap();
    let file=directory.join("config.toml");
    let applied=std::fs::read_to_string(&file).unwrap();
    let value=toml_edit::de::from_str::<Value>(&applied).unwrap();
    let token=value["model_providers"]["ai_manager_service"]["experimental_bearer_token"].as_str().unwrap();
    let c=database::open(&f.paths).unwrap();
    let ledger=query(&c,"SELECT * FROM service_managed_files WHERE instance_id=?1",&[&text(&instance,"instanceId")]).unwrap();
    std::fs::write(&file,"invalid = [").unwrap();
    assert!(service_provider::quick_switch(&f.paths,&targets,&json!({"agentType":"codex","providerId":second})).is_err());
    assert_eq!(service::authorize(&c,token).unwrap()["providerId"],first);
    assert_eq!(query(&c,"SELECT * FROM service_managed_files WHERE instance_id=?1",&[&text(&instance,"instanceId")]).unwrap(),ledger);
    std::fs::write(&file,applied).unwrap();
    service_provider::detach(&f.paths,text(&instance,"instanceId")).unwrap();
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

#[test]
fn service_account_config_backup_omits_local_fields_and_restore_preserves_local_credentials() {
    let source = Fixture::new();
    let codex = save(&source.paths,&json!({"name":"Codex 配置","kind":"codex_account"})).unwrap();
    let google = save(&source.paths,&json!({"name":"Google 配置","kind":"google_account"})).unwrap();
    let codex = text(&codex,"id");
    let google = text(&google,"id");
    let c = database::open(&source.paths).unwrap();
    save_account(&c,codex,&json!({"account_id":"source-account","email":"source@example.test",
        "access_token":"source-access","refresh_token":"source-refresh","expires_at":9999999999000i64})).unwrap();
    let metadata = json!({"subject":"source-subject","email":"google@example.test","port":34567,"project":"source-project"});
    let tokens = json!({"access_token":"source-google-access","refresh_token":"source-google-refresh","expires_at":9999999999i64});
    save_google_credentials(&c,google,&metadata,&tokens).unwrap();
    c.execute("UPDATE service_provider_configs SET quota_json='{\"remaining\":10}' WHERE id=?1",[codex]).unwrap();
    service_provider::switch_backend(&source.paths,"service").unwrap();
    let bytes = database::backup(&source.paths).unwrap();
    let backup_path = source.root.join("account-backup.db");
    std::fs::write(&backup_path,&bytes).unwrap();
    let backup = Connection::open(&backup_path).unwrap();
    for row in query(&backup,"SELECT * FROM service_provider_configs",&[]).unwrap() {
        for column in ACCOUNT_LOCAL_COLUMNS {
            assert_eq!(row[*column],account_local_default(column),"{column}");
        }
    }
    let target = Fixture::new();
    service_provider::switch_backend(&target.paths,"service").unwrap();
    let choices = Map::from_iter([("*".into(),json!("backup"))]);
    database::restore_merged(&target.paths,&bytes,&choices).unwrap();
    let target_c = database::open(&target.paths).unwrap();
    assert!(account(&target_c,codex).is_err());
    assert!(google_tokens(&target_c,google).is_err());
    assert_eq!(config(&target_c,codex).unwrap()["enabled"],0);
    save_account(&target_c,codex,&json!({"account_id":"local-account","access_token":"local-access",
        "refresh_token":"local-refresh","email":"local@example.test"})).unwrap();
    save_google_credentials(&target_c,google,
        &json!({"subject":"local-subject","email":"local-google@example.test","port":34568}),
        &json!({"access_token":"local-google-access","refresh_token":"local-google-refresh","expires_at":9999999999i64})).unwrap();
    let local = query(&target_c,"SELECT * FROM service_provider_configs ORDER BY id",&[]).unwrap();
    // 即使备份中手工混入账号数据，预览也不暴露，覆盖恢复也不写入本机账号字段。
    backup.execute_batch("UPDATE service_provider_configs SET name='备份中的名称',account_id='foreign-account',
        account_email='foreign@example.test',credentials_ciphertext='foreign-secret',credential_generation=999,
        account_metadata_json='{\"project\":\"foreign-project\"}',google_local_key_ciphertext='foreign-local-key';").unwrap();
    drop(backup);
    let tampered = std::fs::read(&backup_path).unwrap();
    let preview = database::preview_rows(&target.paths,&tampered).unwrap();
    assert!(!preview.is_empty());
    for difference in preview {
        assert!(!difference.backup.to_string().contains("foreign-secret"));
        assert!(!difference.current.unwrap_or(Value::Null).to_string().contains("local-access"));
    }
    database::restore_merged(&target.paths,&tampered,&choices).unwrap();
    let restored = query(&target_c,"SELECT * FROM service_provider_configs ORDER BY id",&[]).unwrap();
    for (before,after) in local.iter().zip(restored.iter()) {
        assert_eq!(after["name"],"备份中的名称");
        for column in ACCOUNT_LOCAL_COLUMNS {
            assert_eq!(before[*column],after[*column],"{column}");
        }
    }
    let fresh = Fixture::new();
    service_provider::switch_backend(&fresh.paths,"service").unwrap();
    database::restore_merged(&fresh.paths,&tampered,&choices).unwrap();
    let fresh_c = database::open(&fresh.paths).unwrap();
    assert!(account(&fresh_c,codex).is_err());
    assert!(google_tokens(&fresh_c,google).is_err());
    assert!(query(&fresh_c,"PRAGMA foreign_key_check",&[]).unwrap().is_empty());
}

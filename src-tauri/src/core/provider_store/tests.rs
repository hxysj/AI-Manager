use super::migration::{has_column, table_exists};
use super::*;
use crate::core::paths::resolve_app_paths;
use serde_json::json;
use std::path::Path;

fn fixture() -> (std::path::PathBuf, AppPaths) {
    let root = std::env::temp_dir().join(format!("provider-schema-{}", uuid::Uuid::new_v4()));
    let paths = resolve_app_paths(&root);
    database::initialize(&paths).unwrap();
    (root, paths)
}

fn legacy_database(paths: &AppPaths) {
    let connection = database::open(paths).unwrap();
    for table in [
        "providers",
        "claude_desktop_providers",
        "codex_accounts",
        "provider_models",
        "provider_runtime_profiles",
        "provider_instances",
    ] {
        connection.execute_batch(&format!("CREATE TABLE {table}(item_key TEXT PRIMARY KEY,sort_order INTEGER NOT NULL,payload_json TEXT NOT NULL);")).unwrap();
    }
    connection.execute_batch("CREATE TABLE provider_keys(provider_id TEXT PRIMARY KEY,payload_json TEXT NOT NULL);
      CREATE TABLE provider_runtime_state(cli TEXT PRIMARY KEY,payload_json TEXT NOT NULL);
      CREATE TABLE claude_desktop_settings(setting_key TEXT PRIMARY KEY,payload_json TEXT NOT NULL);
      CREATE TABLE codex_active_account(singleton_id INTEGER PRIMARY KEY,account_id TEXT NOT NULL);
      CREATE TABLE provider_schema_migrations(version INTEGER PRIMARY KEY,applied_at INTEGER NOT NULL);").unwrap();
    for (table, id, value) in [
        (
            "providers",
            "same",
            json!({"id":"same","cli":"claude","name":"API","type":"custom","runtimeConfig":{"mainModel":"model"}}),
        ),
        (
            "claude_desktop_providers",
            "same",
            json!({"id":"same","name":"桌面","mode":"custom","apiFormat":"anthropic"}),
        ),
        (
            "codex_accounts",
            "official",
            json!({"id":"official","type":"codex","email":"a@example.test","access_token":"stale-access","auth":{"accessToken":"encrypted-access","refreshToken":"encrypted-refresh","expiresAt":123}}),
        ),
        (
            "provider_models",
            "model",
            json!({"id":"model","providerId":"same","supportsTools":true}),
        ),
        (
            "provider_runtime_profiles",
            "claude",
            json!({"id":"claude","cli":"claude","providerId":"same","model":"model","env":{"TEST":"yes"}}),
        ),
        (
            "provider_instances",
            "same",
            json!({"id":"same","providerId":"same","profileDir":"profile","sessionsPath":"sessions","updatedAt":1}),
        ),
    ] {
        connection
            .execute(
                &format!("INSERT INTO {table} VALUES(?1,0,?2)"),
                params![id, value.to_string()],
            )
            .unwrap();
    }
    connection.execute("INSERT INTO provider_keys VALUES('same',?1)",[json!({"keys":[{"id":"k1","name":"主","note":"备注","value":"encrypted-1"},{"id":"k2","name":"备","value":"encrypted-2"}],"activeKeyId":"k2"}).to_string()]).unwrap();
    connection
        .execute(
            "INSERT INTO provider_keys VALUES('claude-desktop:same',?1)",
            [json!("encrypted-desktop").to_string()],
        )
        .unwrap();
    connection.execute("INSERT INTO provider_runtime_state VALUES('claude',?1)",[json!({"activeProviderId":"same","status":"SYNCED","runtimeHash":"hash","lastSyncAt":123}).to_string()]).unwrap();
    connection
        .execute(
            "INSERT INTO claude_desktop_settings VALUES('currentProviderId','\"same\"')",
            [],
        )
        .unwrap();
    connection
        .execute("INSERT INTO codex_active_account VALUES(1,'official')", [])
        .unwrap();
}

#[test]
fn migrates_sql_tables_to_explicit_columns_and_individual_keys() {
    let (root, paths) = fixture();
    legacy_database(&paths);
    initialize(&paths).unwrap();
    let connection = database::open(&paths).unwrap();
    for table in OLD_TABLES.iter().copied().chain(["providers_legacy"]) {
        assert!(
            !table_exists(&connection, table).unwrap(),
            "旧表 {table} 应已移除"
        );
    }
    assert!(!has_column(&connection, "providers", "payload_json").unwrap());
    assert!(!has_column(&connection, "providers", "keys").unwrap());
    assert!(!has_column(&connection, "provider_keys", "payload_json").unwrap());
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM providers", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(
        connection
            .query_row("SELECT COUNT(*) FROM provider_keys", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        3
    );
    assert_eq!(read_keys(&paths).unwrap()["same"]["activeKeyId"], "k2");
    assert_eq!(
        read_keys(&paths).unwrap()["same"]["keys"][0]["note"],
        "备注"
    );
    assert_eq!(
        read_keys(&paths).unwrap()["claude-desktop:same"]["keys"][0]["value"],
        "encrypted-desktop"
    );
    assert_eq!(read_models(&paths).unwrap()[0]["supportsTools"], true);
    assert_eq!(read_profiles(&paths).unwrap()[0]["env"]["TEST"], "yes");
    assert_eq!(
        read_runtime_state(&paths).unwrap()["claude"]["runtimeHash"],
        "hash"
    );
    assert_eq!(
        read_codex_accounts(&paths).unwrap()[0]["auth"]["accessToken"],
        "encrypted-access"
    );
    assert_eq!(read_active_codex_account_id(&paths).unwrap(), "official");
    assert_eq!(
        read_desktop_settings(&paths).unwrap()["currentProviderId"],
        "same"
    );
    assert_eq!(
        connection
            .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
            .unwrap(),
        "ok"
    );
    drop(connection);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn migration_failure_restores_all_old_tables() {
    let (root, paths) = fixture();
    legacy_database(&paths);
    let connection = database::open(&paths).unwrap();
    connection
        .execute(
            "INSERT INTO provider_keys VALUES('orphan','\"encrypted\"')",
            [],
        )
        .unwrap();
    assert!(initialize(&paths).is_err());
    assert!(has_column(&connection, "providers", "payload_json").unwrap());
    assert!(has_column(&connection, "provider_keys", "payload_json").unwrap());
    assert!(table_exists(&connection, "codex_accounts").unwrap());
    assert!(!table_exists(&connection, "providers_legacy").unwrap());
    connection
        .execute("DELETE FROM provider_keys WHERE provider_id='orphan'", [])
        .unwrap();
    initialize(&paths).unwrap();
    drop(connection);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn concurrent_initialization_preserves_migrated_keys_models_and_profiles() {
    let (root, paths) = fixture();
    legacy_database(&paths);
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(6));
    std::thread::scope(|scope| {
        let mut handles = Vec::new();
        for _ in 0..6 {
            let barrier = barrier.clone();
            let paths = &paths;
            handles.push(scope.spawn(move || {
                barrier.wait();
                initialize(paths).unwrap();
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
    });
    assert_eq!(
        read_keys(&paths).unwrap()["same"]["keys"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(read_models(&paths).unwrap().len(), 1);
    assert_eq!(read_profiles(&paths).unwrap().len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn key_switch_is_unique_and_delete_provider_cascades() {
    let (root, paths) = fixture();
    write_provider_bundle(
        &paths,
        &[
            json!({"id":"p","cli":"codex"}),
            json!({"id":"other","cli":"claude"}),
        ],
        &[],
        &[],
        &Map::new(),
    )
    .unwrap();
    let mut keys = Map::from_iter([
        (
            "p".into(),
            json!({"keys":[{"id":"a","value":"encrypted-a"},{"id":"b","value":"encrypted-b"}],"activeKeyId":"a"}),
        ),
        ("other".into(), json!("other-key")),
    ]);
    write_keys(&paths, &keys).unwrap();
    keys["p"]["activeKeyId"] = json!("b");
    write_keys(&paths, &keys).unwrap();
    let connection = database::open(&paths).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT id FROM provider_keys WHERE provider_id='p' AND is_active=1",
                [],
                |row| row.get::<_, String>(0)
            )
            .unwrap(),
        "b"
    );
    assert!(connection
        .execute(
            "UPDATE provider_keys SET is_active=1 WHERE provider_id='p' AND id='a'",
            []
        )
        .is_err());
    write_provider_bundle(
        &paths,
        &[
            json!({"id":"p","cli":"codex","name":"修改名称"}),
            json!({"id":"other","cli":"claude"}),
        ],
        &[],
        &[],
        &keys,
    )
    .unwrap();
    assert_eq!(
        read_keys(&paths).unwrap()["p"]["keys"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    write_provider_bundle(
        &paths,
        &[json!({"id":"other","cli":"claude"})],
        &[],
        &[],
        &keys,
    )
    .unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT COUNT(*) FROM provider_keys WHERE provider_id='p'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert!(read_keys(&paths).unwrap().contains_key("other"));
    drop(connection);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn api_and_official_account_activation_is_mutually_exclusive() {
    let (root, paths) = fixture();
    let providers = [
        json!({"id":"p","cli":"codex"}),
        json!({"id":"c","cli":"claude"}),
    ];
    let profiles = [
        json!({"id":"codex","cli":"codex","providerId":"p"}),
        json!({"id":"claude","cli":"claude","providerId":"c"}),
    ];
    write_provider_bundle(&paths, &providers, &[], &profiles, &Map::new()).unwrap();
    write_codex_accounts(&paths, &[json!({"id":"account"})]).unwrap();
    write_active_codex_account_id(&paths, "account").unwrap();
    assert_eq!(read_profiles(&paths).unwrap().len(), 1);
    assert_eq!(read_profiles(&paths).unwrap()[0]["cli"], "claude");
    write_profiles(&paths, &profiles).unwrap();
    assert_eq!(read_active_codex_account_id(&paths).unwrap(), "");
    assert_eq!(read_profiles(&paths).unwrap().len(), 2);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn saving_unchanged_bundle_does_not_write_database_pages() {
    let (root, paths) = fixture();
    let providers = [json!({"id":"p","cli":"codex","name":"供应商"})];
    let profiles = [json!({"id":"codex","cli":"codex","providerId":"p","model":"test"})];
    let keys = Map::from_iter([(
        "p".into(),
        json!({"keys":[{"id":"k","value":"encrypted"}],"activeKeyId":"k"}),
    )]);
    write_provider_bundle(&paths, &providers, &[], &profiles, &keys).unwrap();
    let connection = database::open(&paths).unwrap();
    let before: i64 = connection
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap();
    write_provider_bundle(&paths, &providers, &[], &profiles, &keys).unwrap();
    let after: i64 = connection
        .query_row("PRAGMA data_version", [], |row| row.get(0))
        .unwrap();
    assert_eq!(before, after);
    drop(connection);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn migrates_provider_json_into_main_database() {
    let root = std::env::temp_dir().join(format!(
        "monkey-thief-provider-store-{}",
        std::process::id()
    ));
    if root.exists() {
        std::fs::remove_dir_all(&root).unwrap();
    }
    let paths = resolve_app_paths(Path::new(&root));
    std::fs::create_dir_all(&paths.storage_dir).unwrap();

    for (path, payload) in [
        (
            &paths.storage_files.providers,
            json!([{"id": "provider-a", "cli":"claude"}]),
        ),
        (
            &paths.storage_files.runtime_models,
            json!([{"id": "model-a", "providerId": "provider-a"}]),
        ),
        (
            &paths.storage_files.runtime_profiles,
            json!([{"id": "claude", "cli": "claude", "providerId": "provider-a"}]),
        ),
        (
            &paths.storage_files.runtime_provider_keys,
            json!({"provider-a": "encrypted-key"}),
        ),
        (
            &paths.storage_files.runtime_provider_state,
            json!({"claude": {"activeProviderId":"provider-a", "status": "SYNCED"}}),
        ),
        (
            &paths.storage_files.codex_provider_instances,
            json!([{"id": "provider-a", "providerId": "provider-a"}]),
        ),
        (
            &paths.storage_files.codex_accounts,
            json!([{"id": "account-a", "email": "test@example.com"}]),
        ),
        (
            &paths.storage_files.codex_active_account_id,
            json!("account-a"),
        ),
    ] {
        std::fs::write(path, serde_json::to_string(&payload).unwrap()).unwrap();
    }

    assert_eq!(read_providers(&paths).unwrap().len(), 1);
    assert_eq!(read_models(&paths).unwrap().len(), 1);
    assert_eq!(read_profiles(&paths).unwrap().len(), 1);
    assert_eq!(
        read_keys(&paths).unwrap()["provider-a"]["keys"][0]["value"],
        "encrypted-key"
    );
    assert_eq!(
        read_runtime_state(&paths).unwrap()["claude"]["status"],
        "SYNCED"
    );
    assert_eq!(read_instances(&paths).unwrap().len(), 1);
    assert_eq!(read_codex_accounts(&paths).unwrap().len(), 1);
    assert_eq!(read_active_codex_account_id(&paths).unwrap(), "account-a");

    for path in [
        &paths.storage_files.providers,
        &paths.storage_files.runtime_models,
        &paths.storage_files.runtime_profiles,
        &paths.storage_files.runtime_provider_keys,
        &paths.storage_files.runtime_provider_state,
        &paths.storage_files.codex_provider_instances,
        &paths.storage_files.codex_accounts,
        &paths.storage_files.codex_active_account_id,
    ] {
        assert!(!Path::new(path).exists());
    }
}

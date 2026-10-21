-- 来自 0be86b3f196f557743889d28bc93fa92bbce8a5f 的真实建表与索引定义。
CREATE TABLE IF NOT EXISTS app_metadata (
           key TEXT PRIMARY KEY,
           value TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS provider_schema_migrations (
           version INTEGER PRIMARY KEY,
           applied_at INTEGER NOT NULL
         );

CREATE TABLE IF NOT EXISTS providers (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS claude_desktop_providers (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS claude_desktop_settings (
           setting_key TEXT PRIMARY KEY,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS provider_models (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS provider_runtime_profiles (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS provider_keys (
           provider_id TEXT PRIMARY KEY,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS provider_runtime_state (
           cli TEXT PRIMARY KEY,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS provider_instances (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS codex_accounts (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS codex_active_account (
           singleton_id INTEGER PRIMARY KEY CHECK(singleton_id = 1),
           account_id TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS skill_schema_migrations (
           version INTEGER PRIMARY KEY,
           applied_at INTEGER NOT NULL
         );

CREATE TABLE IF NOT EXISTS skills (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS skill_groups (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS skill_repositories (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS skill_repository_cache (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS skill_installs (
           skill_name TEXT PRIMARY KEY,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS skill_trash (
           item_key TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS rule_schema_migrations (
           version INTEGER PRIMARY KEY,
           applied_at INTEGER NOT NULL
         );

CREATE TABLE IF NOT EXISTS rule_prompts (
           prompt_id TEXT PRIMARY KEY,
           cli TEXT NOT NULL,
           file_name TEXT NOT NULL,
           updated_at INTEGER NOT NULL,
           payload_json TEXT NOT NULL
         );

CREATE INDEX IF NOT EXISTS idx_rule_prompts_cli_updated_at
           ON rule_prompts(cli, updated_at DESC);

CREATE TABLE IF NOT EXISTS rule_profiles (
           cli TEXT PRIMARY KEY,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS rule_runtime_state (
           cli TEXT PRIMARY KEY,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS schema_migrations (
           version INTEGER PRIMARY KEY,
           applied_at INTEGER NOT NULL
         );

CREATE TABLE IF NOT EXISTS usage_metadata (
           key TEXT PRIMARY KEY,
           value TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS usage_logs (
           request_id TEXT PRIMARY KEY,
           raw_path TEXT NOT NULL DEFAULT '',
           created_at INTEGER NOT NULL,
           session_updated_at INTEGER NOT NULL DEFAULT 0,
           app_type TEXT NOT NULL DEFAULT '',
           provider_id TEXT NOT NULL DEFAULT '',
           model TEXT NOT NULL DEFAULT '',
           request_source TEXT NOT NULL DEFAULT 'session',
           payload_json TEXT NOT NULL
         );

CREATE INDEX IF NOT EXISTS idx_usage_logs_created_at
           ON usage_logs(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_usage_logs_raw_path
           ON usage_logs(raw_path);

CREATE INDEX IF NOT EXISTS idx_usage_logs_app_created_at
           ON usage_logs(app_type, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_usage_logs_provider_created_at
           ON usage_logs(provider_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_usage_logs_model_created_at
           ON usage_logs(model, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_usage_logs_source_created_at
           ON usage_logs(request_source, created_at DESC);

CREATE TABLE IF NOT EXISTS usage_sessions (
           raw_path TEXT PRIMARY KEY,
           app_type TEXT NOT NULL DEFAULT '',
           updated_at INTEGER NOT NULL DEFAULT 0
         );

CREATE TABLE IF NOT EXISTS usage_request_records (
           request_id TEXT PRIMARY KEY,
           created_at INTEGER NOT NULL DEFAULT 0,
           payload_json TEXT NOT NULL
         );

CREATE INDEX IF NOT EXISTS idx_usage_request_records_created_at
           ON usage_request_records(created_at DESC);

CREATE TABLE IF NOT EXISTS skill_usage_session_records (
           raw_path TEXT PRIMARY KEY,
           updated_at INTEGER NOT NULL DEFAULT 0,
           payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS usage_pricing_config (
           id INTEGER PRIMARY KEY CHECK(id = 1),
           exchange_rate REAL NOT NULL
         );

CREATE TABLE IF NOT EXISTS usage_pricing_items (
           id TEXT PRIMARY KEY,
           sort_order INTEGER NOT NULL,
           model_id TEXT NOT NULL,
           model_category TEXT NOT NULL DEFAULT '',
           currency TEXT NOT NULL DEFAULT 'USD',
           input_cost_per_million REAL NOT NULL DEFAULT 0,
           output_cost_per_million REAL NOT NULL DEFAULT 0,
           cache_read_cost_per_million REAL NOT NULL DEFAULT 0,
           cache_creation_cost_per_million REAL NOT NULL DEFAULT 0
         );

CREATE INDEX IF NOT EXISTS idx_usage_pricing_model_id
           ON usage_pricing_items(model_id);

CREATE TABLE IF NOT EXISTS codex_quota_stages (
           stage_id TEXT PRIMARY KEY,
           account_id TEXT NOT NULL,
           window_key TEXT NOT NULL,
           reset_at INTEGER NOT NULL,
           starts_at INTEGER NOT NULL DEFAULT 0,
           observed_at INTEGER NOT NULL,
           completed_at INTEGER NOT NULL DEFAULT 0,
           payload_json TEXT NOT NULL,
           UNIQUE(account_id, window_key, reset_at)
         );

CREATE INDEX IF NOT EXISTS idx_codex_quota_stages_account_reset
           ON codex_quota_stages(account_id, reset_at DESC);

CREATE TABLE IF NOT EXISTS provider_key_usage (
           provider_id TEXT NOT NULL,
           key_id TEXT NOT NULL,
           key_hash TEXT NOT NULL,
           request_count INTEGER NOT NULL DEFAULT 0,
           success_count INTEGER NOT NULL DEFAULT 0,
           failure_count INTEGER NOT NULL DEFAULT 0,
           consecutive_failures INTEGER NOT NULL DEFAULT 0,
           last_used_at INTEGER NOT NULL DEFAULT 0,
           last_success_at INTEGER NOT NULL DEFAULT 0,
           last_failure_at INTEGER NOT NULL DEFAULT 0,
           last_status_code INTEGER,
           last_error_kind TEXT NOT NULL DEFAULT '',
           last_failure_kind TEXT NOT NULL DEFAULT '',
           last_outcome TEXT NOT NULL DEFAULT '',
           PRIMARY KEY(provider_id, key_id, key_hash)
         );

CREATE TABLE IF NOT EXISTS provider_key_bindings (
        provider_id TEXT NOT NULL, observed_at INTEGER NOT NULL, payload_json TEXT NOT NULL,
        PRIMARY KEY(provider_id, observed_at)
    );

CREATE TABLE IF NOT EXISTS image_tasks (
           id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, status TEXT NOT NULL,
           payload_json TEXT NOT NULL, thumbnail TEXT NOT NULL DEFAULT ''
         );

CREATE TABLE IF NOT EXISTS image_outputs (
           task_id TEXT NOT NULL, position INTEGER NOT NULL, format TEXT NOT NULL,
           bytes BLOB NOT NULL, PRIMARY KEY(task_id, position)
         );

CREATE INDEX IF NOT EXISTS idx_image_tasks_created ON image_tasks(created_at DESC);

CREATE TABLE IF NOT EXISTS image_inputs (
           task_id TEXT NOT NULL, position INTEGER NOT NULL, data_url TEXT NOT NULL,
           PRIMARY KEY(task_id, position)
         );

CREATE INDEX IF NOT EXISTS idx_image_tasks_status ON image_tasks(status);

CREATE TABLE IF NOT EXISTS image_web_conversations (
           id TEXT PRIMARY KEY, account_id TEXT NOT NULL,
           local_conversation_id TEXT NOT NULL, conversation_id TEXT NOT NULL,
           parent_message_id TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL,
           updated_at INTEGER NOT NULL, deleted_at INTEGER,
           is_deleted INTEGER NOT NULL DEFAULT 0
         );

CREATE UNIQUE INDEX IF NOT EXISTS idx_image_web_conversations_upstream
           ON image_web_conversations(account_id, conversation_id);

CREATE INDEX IF NOT EXISTS idx_image_web_conversations_local
           ON image_web_conversations(account_id, local_conversation_id, is_deleted, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_image_web_conversations_reuse
           ON image_web_conversations(account_id, is_deleted, updated_at DESC);

CREATE TABLE IF NOT EXISTS image_prompt_templates (
           id TEXT PRIMARY KEY, position INTEGER NOT NULL, search_text TEXT NOT NULL,
           summary_json TEXT NOT NULL, payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS image_prompt_categories (
           template_id TEXT NOT NULL, category_id TEXT NOT NULL,
           PRIMARY KEY(template_id, category_id)
         );

CREATE INDEX IF NOT EXISTS idx_image_prompt_category ON image_prompt_categories(category_id, template_id);

CREATE TABLE IF NOT EXISTS image_prompt_catalog (id INTEGER PRIMARY KEY, payload_json TEXT NOT NULL);

CREATE TABLE IF NOT EXISTS translation_history (
           id TEXT PRIMARY KEY, created_at INTEGER NOT NULL, payload_json TEXT NOT NULL
         );

CREATE TABLE IF NOT EXISTS translation_requests (
           id TEXT PRIMARY KEY, translation_id TEXT NOT NULL,
           created_at INTEGER NOT NULL, payload_json TEXT NOT NULL
         );

CREATE INDEX IF NOT EXISTS idx_translation_history_created ON translation_history(created_at DESC);

CREATE INDEX IF NOT EXISTS idx_translation_requests_history ON translation_requests(translation_id, created_at DESC);

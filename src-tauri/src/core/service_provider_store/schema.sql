-- 服务商配置与旧 Provider 存储独立，所有删除均保留请求历史。
CREATE TABLE IF NOT EXISTS service_provider_configs (
  id TEXT PRIMARY KEY NOT NULL,
  name TEXT NOT NULL CHECK(length(trim(name))>0),
  icon TEXT NOT NULL DEFAULT '',
  kind TEXT NOT NULL CHECK(kind IN ('api','codex_account')),
  upstream_protocol TEXT NOT NULL CHECK(upstream_protocol IN ('responses','chat')),
  base_url TEXT NOT NULL DEFAULT '',
  proxy TEXT NOT NULL DEFAULT '',
  headers_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(headers_json) AND json_type(headers_json)='object'),
  note TEXT NOT NULL DEFAULT '',
  enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
  revision INTEGER NOT NULL DEFAULT 1 CHECK(revision>0),
  deleted_at INTEGER,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  CHECK(kind<>'codex_account' OR upstream_protocol='responses')
) STRICT;
CREATE TABLE IF NOT EXISTS service_provider_keys (
  id TEXT PRIMARY KEY NOT NULL,
  provider_id TEXT NOT NULL REFERENCES service_provider_configs(id) ON DELETE RESTRICT,
  logical_id TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  note TEXT NOT NULL DEFAULT '',
  secret_ciphertext TEXT NOT NULL DEFAULT '',
  encryption_version INTEGER NOT NULL DEFAULT 1 CHECK(encryption_version=1),
  fingerprint TEXT NOT NULL,
  is_current INTEGER NOT NULL DEFAULT 1 CHECK(is_current IN (0,1)),
  is_active INTEGER NOT NULL DEFAULT 0 CHECK(is_active IN (0,1)),
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','archived','unreadable','revoked')),
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(provider_id,logical_id,fingerprint),
  UNIQUE(provider_id,id),
  CHECK(is_current=1 OR (is_active=0 AND secret_ciphertext=''))
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS service_keys_current ON service_provider_keys(provider_id,logical_id) WHERE is_current=1;
CREATE UNIQUE INDEX IF NOT EXISTS service_keys_active ON service_provider_keys(provider_id) WHERE is_active=1;
CREATE TABLE IF NOT EXISTS service_provider_accounts (
  id TEXT PRIMARY KEY NOT NULL,
  provider_id TEXT NOT NULL UNIQUE REFERENCES service_provider_configs(id) ON DELETE RESTRICT,
  external_account_id TEXT NOT NULL,
  email TEXT NOT NULL DEFAULT '',
  access_ciphertext TEXT NOT NULL DEFAULT '',
  refresh_ciphertext TEXT NOT NULL DEFAULT '',
  id_ciphertext TEXT NOT NULL DEFAULT '',
  expires_at INTEGER,
  token_generation INTEGER NOT NULL DEFAULT 0,
  refresh_status TEXT NOT NULL DEFAULT '',
  quota_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(quota_json)),
  revision INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE TABLE IF NOT EXISTS service_provider_models (
  id TEXT PRIMARY KEY NOT NULL,
  provider_id TEXT NOT NULL REFERENCES service_provider_configs(id) ON DELETE RESTRICT,
  upstream_model_id TEXT NOT NULL CHECK(length(trim(upstream_model_id))>0),
  display_name TEXT NOT NULL DEFAULT '',
  context_window INTEGER CHECK(context_window>0),
  catalog_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(catalog_json) AND json_type(catalog_json)='object'),
  catalog_schema_version INTEGER NOT NULL DEFAULT 1,
  enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
  input_price_micros INTEGER CHECK(input_price_micros>=0),
  cached_input_price_micros INTEGER CHECK(cached_input_price_micros>=0),
  output_price_micros INTEGER CHECK(output_price_micros>=0),
  currency TEXT NOT NULL DEFAULT 'USD',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  UNIQUE(provider_id,upstream_model_id),
  UNIQUE(provider_id,id)
) STRICT;
CREATE TABLE IF NOT EXISTS service_provider_agent_bindings (
  id TEXT PRIMARY KEY NOT NULL DEFAULT (lower(hex(randomblob(16)))),
  provider_id TEXT NOT NULL REFERENCES service_provider_configs(id) ON DELETE RESTRICT,
  agent_type TEXT NOT NULL CHECK(length(agent_type)>0),
  enabled INTEGER NOT NULL DEFAULT 0 CHECK(enabled IN (0,1)),
  default_model_id TEXT,
  options_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(options_json) AND json_type(options_json)='object'),
  revision INTEGER NOT NULL DEFAULT 1,
  updated_at INTEGER NOT NULL,
  UNIQUE(provider_id,agent_type),
  FOREIGN KEY(provider_id,default_model_id) REFERENCES service_provider_models(provider_id,id) ON DELETE RESTRICT
) STRICT;
CREATE TABLE IF NOT EXISTS service_agent_model_routes (
  id TEXT PRIMARY KEY NOT NULL,
  provider_id TEXT NOT NULL,
  agent_type TEXT NOT NULL,
  client_model_name TEXT NOT NULL CHECK(length(client_model_name)>0),
  model_id TEXT NOT NULL,
  enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN (0,1)),
  FOREIGN KEY(provider_id,agent_type) REFERENCES service_provider_agent_bindings(provider_id,agent_type) ON DELETE RESTRICT,
  FOREIGN KEY(provider_id,model_id) REFERENCES service_provider_models(provider_id,id) ON DELETE RESTRICT,
  UNIQUE(provider_id,agent_type,client_model_name)
) STRICT;
CREATE TABLE IF NOT EXISTS service_compatibility_tests (
  id TEXT PRIMARY KEY NOT NULL,
  provider_id TEXT NOT NULL,
  agent_type TEXT NOT NULL,
  model_id TEXT NOT NULL,
  provider_revision INTEGER NOT NULL,
  agent_version TEXT NOT NULL,
  adapter_version INTEGER NOT NULL DEFAULT 1,
  client_protocol TEXT NOT NULL,
  upstream_protocol TEXT NOT NULL,
  capability TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('passed','failed','unsupported')),
  evidence_json TEXT NOT NULL CHECK(json_valid(evidence_json)),
  tested_at INTEGER NOT NULL,
  FOREIGN KEY(provider_id,agent_type) REFERENCES service_provider_agent_bindings(provider_id,agent_type) ON DELETE RESTRICT,
  FOREIGN KEY(provider_id,model_id) REFERENCES service_provider_models(provider_id,id) ON DELETE RESTRICT
) STRICT;
CREATE TABLE IF NOT EXISTS service_agent_instances (
  id TEXT PRIMARY KEY NOT NULL,
  agent_type TEXT NOT NULL CHECK(length(agent_type)>0),
  name TEXT NOT NULL,
  config_path TEXT NOT NULL,
  agent_version TEXT NOT NULL DEFAULT '',
  client_protocol TEXT NOT NULL CHECK(client_protocol IN ('responses','messages','chat','all')),
  provider_id TEXT,
  model_id TEXT,
  status TEXT NOT NULL DEFAULT 'detached' CHECK(status IN ('detached','active','error')),
  sync_status TEXT NOT NULL DEFAULT 'pending' CHECK(sync_status IN ('pending','synced','conflict','restored')),
  applied_revision INTEGER,
  applied_hash TEXT NOT NULL DEFAULT '',
  last_synced_at INTEGER,
  last_error TEXT NOT NULL DEFAULT '',
  revision INTEGER NOT NULL DEFAULT 1,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY(provider_id,model_id) REFERENCES service_provider_models(provider_id,id) ON DELETE RESTRICT,
  FOREIGN KEY(provider_id,agent_type) REFERENCES service_provider_agent_bindings(provider_id,agent_type) ON DELETE RESTRICT,
  CHECK(status<>'active' OR (provider_id IS NOT NULL AND model_id IS NOT NULL)),
  CHECK((provider_id IS NULL)=(model_id IS NULL)),
  UNIQUE(agent_type,config_path)
) STRICT;
CREATE TABLE IF NOT EXISTS service_gateway_tokens (
  id TEXT PRIMARY KEY NOT NULL,
  token_hash TEXT NOT NULL UNIQUE,
  token_prefix TEXT NOT NULL,
  instance_id TEXT NOT NULL REFERENCES service_agent_instances(id) ON DELETE RESTRICT,
  status TEXT NOT NULL DEFAULT 'active' CHECK(status IN ('active','revoked','expired')),
  expires_at INTEGER,
  last_used_at INTEGER,
  created_at INTEGER NOT NULL,
  revoked_at INTEGER
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS service_token_current ON service_gateway_tokens(instance_id) WHERE status='active';
CREATE TABLE IF NOT EXISTS service_switch_operations (
  id TEXT PRIMARY KEY NOT NULL,
  from_backend TEXT NOT NULL CHECK(from_backend IN ('legacy','service')),
  to_backend TEXT NOT NULL CHECK(to_backend IN ('legacy','service')),
  generation INTEGER NOT NULL,
  stage TEXT NOT NULL,
  error_message TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  completed_at INTEGER
) STRICT;
CREATE TABLE IF NOT EXISTS service_managed_files (
  id TEXT PRIMARY KEY NOT NULL,
  operation_id TEXT NOT NULL,
  instance_id TEXT NOT NULL REFERENCES service_agent_instances(id) ON DELETE RESTRICT,
  file_path TEXT NOT NULL,
  before_exists INTEGER NOT NULL CHECK(before_exists IN (0,1)),
  before_sha256 TEXT NOT NULL,
  before_ciphertext TEXT NOT NULL,
  after_sha256 TEXT NOT NULL,
  after_ciphertext TEXT NOT NULL,
  managed_fields_json TEXT NOT NULL CHECK(json_valid(managed_fields_json)),
  write_status TEXT NOT NULL CHECK(write_status IN ('prepared','written','failed')),
  restore_status TEXT NOT NULL DEFAULT 'pending' CHECK(restore_status IN ('pending','restored','conflict')),
  error_message TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS service_managed_file_owner ON service_managed_files(file_path) WHERE restore_status<>'restored';
CREATE TABLE IF NOT EXISTS service_gateway_requests (
  id TEXT PRIMARY KEY NOT NULL,
  token_id TEXT,
  instance_id TEXT,
  agent_type TEXT NOT NULL DEFAULT '',
  generation INTEGER NOT NULL,
  source TEXT NOT NULL DEFAULT 'service-gateway',
  client_request_id TEXT NOT NULL DEFAULT '',
  requested_model TEXT NOT NULL DEFAULT '',
  protocol_in TEXT NOT NULL,
  request_path TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('received','rejected','routed','completed','failed','cancelled','unknown')),
  status_code INTEGER,
  error_message TEXT NOT NULL DEFAULT '',
  created_at INTEGER NOT NULL,
  finished_at INTEGER
) STRICT;
CREATE INDEX IF NOT EXISTS service_requests_created ON service_gateway_requests(created_at);
CREATE TABLE IF NOT EXISTS service_gateway_attempts (
  id TEXT PRIMARY KEY NOT NULL,
  request_id TEXT NOT NULL REFERENCES service_gateway_requests(id) ON DELETE RESTRICT,
  attempt_no INTEGER NOT NULL CHECK(attempt_no>0),
  provider_id TEXT NOT NULL REFERENCES service_provider_configs(id) ON DELETE RESTRICT,
  provider_name TEXT NOT NULL,
  credential_id TEXT NOT NULL,
  credential_name TEXT NOT NULL DEFAULT '',
  credential_fingerprint TEXT NOT NULL DEFAULT '',
  model_id TEXT NOT NULL,
  upstream_model TEXT NOT NULL,
  protocol_out TEXT NOT NULL,
  state TEXT NOT NULL CHECK(state IN ('created','started','streaming','succeeded','failed','cancelled','unknown')),
  status_code INTEGER,
  upstream_request_id TEXT NOT NULL DEFAULT '',
  upstream_response_id TEXT NOT NULL DEFAULT '',
  actual_model TEXT NOT NULL DEFAULT '',
  service_tier TEXT NOT NULL DEFAULT '',
  usage_source TEXT NOT NULL DEFAULT 'unknown' CHECK(usage_source IN ('unknown','upstream')),
  input_tokens INTEGER CHECK(input_tokens>=0),
  output_tokens INTEGER CHECK(output_tokens>=0),
  cached_input_tokens INTEGER CHECK(cached_input_tokens>=0),
  reasoning_tokens INTEGER CHECK(reasoning_tokens>=0),
  price_snapshot_json TEXT NOT NULL DEFAULT '{}' CHECK(json_valid(price_snapshot_json)),
  estimated_cost_micros INTEGER CHECK(estimated_cost_micros>=0),
  currency TEXT NOT NULL DEFAULT 'USD',
  error_message TEXT NOT NULL DEFAULT '',
  started_at INTEGER NOT NULL,
  finished_at INTEGER,
  UNIQUE(request_id,attempt_no),
  FOREIGN KEY(provider_id,model_id) REFERENCES service_provider_models(provider_id,id) ON DELETE RESTRICT
) STRICT;
CREATE TABLE IF NOT EXISTS service_provider_migrations (
  id TEXT PRIMARY KEY NOT NULL,
  source_schema TEXT NOT NULL,
  status TEXT NOT NULL CHECK(status IN ('running','completed','failed')),
  created_at INTEGER NOT NULL,
  completed_at INTEGER,
  error_message TEXT NOT NULL DEFAULT ''
) STRICT;
CREATE TABLE IF NOT EXISTS service_provider_migration_items (
  id TEXT PRIMARY KEY NOT NULL,
  migration_id TEXT NOT NULL REFERENCES service_provider_migrations(id) ON DELETE RESTRICT,
  source_table TEXT NOT NULL,
  source_id TEXT NOT NULL,
  source_fingerprint TEXT NOT NULL,
  target_id TEXT,
  status TEXT NOT NULL CHECK(status IN ('copied','skipped','unreadable','already_copied')),
  reason TEXT NOT NULL DEFAULT '',
  UNIQUE(migration_id,source_table,source_id)
) STRICT;
CREATE UNIQUE INDEX IF NOT EXISTS service_migration_source ON service_provider_migration_items(source_table,source_id) WHERE status IN ('copied','unreadable');

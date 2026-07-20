use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use crate::core::settings::string_value;
use crate::core::usage_store::{self, UsageLogQuery, UsageSessionUpdate};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use chrono::{Datelike, Local, TimeZone, Timelike};
use regex::Regex;
use serde_json::{json, Map, Value};
use sha1::{Digest, Sha1};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

const DEFAULT_EXCHANGE_RATE: f64 = 7.2;
static PRICING_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
static USAGE_LOG_CACHE: OnceLock<Mutex<Option<UsageLogCache>>> = OnceLock::new();
static USAGE_PROVIDER_STATS_CACHE: OnceLock<Mutex<HashMap<String, UsageProviderStatsCache>>> =
    OnceLock::new();

#[derive(Clone)]
struct Summary {
    request_count: u64,
    usage_count: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    actual_tokens: u64,
    total_cost_usd: f64,
    last_used_at: u64,
}

#[derive(Clone)]
struct GroupStat {
    base: Map<String, Value>,
    summary: Summary,
}

#[derive(Clone)]
struct SkillInfo {
    name: String,
    description: String,
    source_paths: Vec<String>,
    cli_types: Vec<Value>,
    aliases: Vec<String>,
}

#[derive(Clone)]
struct SkillInvocation {
    skill_name: String,
    cli: String,
    raw_path: String,
    created_at: u64,
}

struct UsageLogCache {
    path: String,
    revision: u64,
    logs: Arc<Vec<Value>>,
}

struct UsageProviderStatsCache {
    path: String,
    logs: Arc<Vec<Value>>,
    today_start_at: u64,
    log_signatures: HashMap<String, String>,
    summary: Summary,
    today_summary: Summary,
    model_stats: HashMap<String, GroupStat>,
    today_model_stats: HashMap<String, GroupStat>,
}

pub fn build_state(paths: &AppPaths) -> Result<Value, ManagerError> {
    get_initial_state_data(paths)
}

pub async fn get_stats(paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
    Ok(json!({
      "status": "ok",
      "data": get_stats_data(paths, payload)?,
      "message": ""
    }))
}

pub async fn sync_usage(
    paths: &AppPaths,
    payload: Value,
    state: &Value,
) -> Result<Value, ManagerError> {
    let diagnostics = refresh_usage(paths, state).await?;
    let data = get_stats_data(paths, payload)?;

    Ok(json!({
      "status": "ok",
      "data": data,
      "message": "",
      "diagnostics": diagnostics
    }))
}

pub async fn get_skill_usage_stats(
    paths: &AppPaths,
    payload: Value,
    state: &Value,
) -> Result<Value, ManagerError> {
    let cli_targets = state
        .get("cliTargets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let managed_skills = state
        .get("skills")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let usage_logs = get_stats_data(paths, json!({}))?["logs"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let skills = collect_skills(&cli_targets, &managed_skills).await?;
    let alias_map = create_alias_map(&skills);
    let files = collect_cli_session_files(&cli_targets)?;
    let mut diagnostics = Vec::new();
    let mut invocations = Vec::new();
    let filters = json!({
      "cli": non_empty_text(payload.get("cli"), "all"),
      "startAt": number_value(payload.get("startAt"), 0),
      "endAt": number_value(payload.get("endAt"), 0),
      "trendMode": normalize_skill_trend_mode(payload.get("trendMode"))
    });

    for item in files {
        let records = match read_session_records(&item) {
            Ok(records) => records,
            Err(error) => {
                diagnostics.push(json!({
                  "type": "skill-usage-parse-error",
                  "message": error.to_string(),
                  "sourcePath": string_value(item.get("filePath"))
                }));
                continue;
            }
        };

        for record in records {
            let display = string_value(record.get("display")).trim().to_string();
            let skill_names = extract_skill_names(&display, &alias_map);

            if skill_names.is_empty() {
                continue;
            }

            let created_at = to_timestamp_ms(record.get("timestamp"), 0);

            for skill_name in skill_names {
                invocations.push(SkillInvocation {
                    skill_name,
                    cli: string_value(item.get("cli")),
                    raw_path: string_value(record.get("rawPath")),
                    created_at,
                });
            }
        }
    }

    let usage_logs = usage_logs
        .into_iter()
        .filter(|log| match_log_filters(log, &filters))
        .collect::<Vec<_>>();
    let filtered_invocations = invocations
        .iter()
        .enumerate()
        .filter(|(_, item)| match_invocation_filters(item, &filters))
        .map(|(index, item)| (index, item.clone()))
        .collect::<Vec<_>>();
    let matched_logs = match_invocation_logs(&filtered_invocations, &usage_logs);
    let rows = create_skill_rows(&skills, &invocations, &matched_logs, &filters);
    let trends = create_skill_trend_stats(
        &filtered_invocations
            .iter()
            .map(|(_, item)| item.clone())
            .collect::<Vec<_>>(),
        &filters,
    );
    let summary = rows.iter().fold(
        json!({
          "skillCount": 0,
          "usedSkillCount": 0,
          "usageCount": 0,
          "requestCount": 0,
          "actualTokens": 0,
          "totalCostUsd": 0.0,
          "lastUsedAt": 0
        }),
        |mut result, item| {
            result["skillCount"] = json!(number_value(result.get("skillCount"), 0) + 1);
            result["usageCount"] = json!(
                number_value(result.get("usageCount"), 0) + number_value(item.get("usageCount"), 0)
            );
            result["requestCount"] = json!(
                number_value(result.get("requestCount"), 0)
                    + number_value(item.get("requestCount"), 0)
            );
            result["actualTokens"] = json!(
                number_value(result.get("actualTokens"), 0)
                    + number_value(item.get("actualTokens"), 0)
            );
            result["totalCostUsd"] = json!(
                price_number(result.get("totalCostUsd"), 0.0)
                    + price_number(item.get("totalCostUsd"), 0.0)
            );
            result["lastUsedAt"] = json!(number_value(result.get("lastUsedAt"), 0)
                .max(number_value(item.get("lastUsedAt"), 0)));

            if number_value(item.get("usageCount"), 0) > 0 {
                result["usedSkillCount"] = json!(number_value(result.get("usedSkillCount"), 0) + 1);
            }

            result
        },
    );

    Ok(json!({
      "status": "ok",
      "data": {
        "summary": summary,
        "skills": rows,
        "trends": trends,
        "filters": {
          "clis": cli_filter_options(&cli_targets)
        },
        "diagnostics": diagnostics
      },
      "message": ""
    }))
}

pub async fn get_pricing(paths: &AppPaths) -> Result<Value, ManagerError> {
    Ok(json!({
      "status": "ok",
      "data": read_pricing(paths)?,
      "message": ""
    }))
}

pub async fn save_pricing(paths: &AppPaths, payload: Value) -> Result<Value, ManagerError> {
    let pricing = normalize_pricing_config(payload)?;

    write_pricing(paths, &pricing).await?;
    Ok(json!({
      "status": "ok",
      "data": pricing,
      "message": ""
    }))
}

pub async fn export_report_image(payload: Value) -> Result<Value, ManagerError> {
    let target_path = string_value(payload.get("targetPath"));

    if target_path.is_empty() {
        return Ok(json!({
          "status": "ok",
          "data": {
            "canceled": true
          },
          "message": ""
        }));
    }

    let image_data = string_value(payload.get("imageData"));
    let image_bytes = decode_report_image_data_url(&image_data)?;

    std::fs::write(&target_path, image_bytes)?;

    Ok(json!({
      "status": "ok",
      "data": {
        "canceled": false,
        "targetPath": target_path
      },
      "message": ""
    }))
}

fn get_stats_data(paths: &AppPaths, input: Value) -> Result<Value, ManagerError> {
    let pricing_config = read_pricing(paths)?;
    let pricing_index = create_pricing_index(&pricing_config);
    let filters = json!({
      "appType": normalize_app_type(&non_empty_text(input.get("appType"), "all")),
      "providerId": non_empty_text(input.get("providerId"), "all"),
      "providerIds": string_array(input.get("providerIds")),
      "model": non_empty_text(input.get("model"), "all"),
      "requestSource": non_empty_text(input.get("requestSource"), "all"),
      "startAt": number_value(input.get("startAt"), 0),
      "endAt": number_value(input.get("endAt"), 0),
      "trendMode": normalize_usage_trend_mode(input.get("trendMode"))
    });

    if input.get("statsScope").and_then(Value::as_str) == Some("provider") {
        return get_provider_stats_data(paths, &pricing_config, &pricing_index, &filters);
    }

    let log_page = number_value(input.get("logPage"), 1).max(1) as usize;
    let log_page_size = number_value(input.get("logPageSize"), 20).max(1) as usize;
    let include_all_logs = input.get("includeAllLogs").and_then(Value::as_bool) == Some(true);
    let selected_logs = usage_store::query_logs(paths, &create_usage_log_query(&filters))?;
    let option_filters = json!({
      "appType": filters["appType"],
      "providerId": "all",
      "providerIds": filters["providerIds"],
      "model": "all",
      "requestSource": "all",
      "startAt": filters["startAt"],
      "endAt": filters["endAt"],
      "trendMode": filters["trendMode"]
    });
    let option_logs = if filters["providerId"] == "all"
        && filters["providerIds"]
            .as_array()
            .is_none_or(|items| items.is_empty())
        && filters["model"] == "all"
        && filters["requestSource"] == "all"
    {
        selected_logs.clone()
    } else {
        usage_store::query_logs(paths, &create_usage_log_query(&option_filters))?
    };
    let logs = selected_logs
        .iter()
        .map(|item| enrich_usage_log(item, &pricing_config, &pricing_index))
        .collect::<Vec<_>>();
    let mut summary = create_empty_summary();

    for log in &logs {
        append_usage_summary(&mut summary, log);
    }

    let trend_mode = string_value(filters.get("trendMode"));
    let trends = create_usage_trend_stats(&logs, &trend_mode, &filters);
    let model_trend_series = create_usage_model_trend_series(&logs, &trend_mode, &filters, &trends);
    let provider_trend_series =
        create_usage_provider_trend_series(&logs, &trend_mode, &filters, &trends);
    let log_total_count = logs.len();
    let response_logs = if include_all_logs {
        logs.clone()
    } else {
        logs.iter()
            .skip((log_page - 1) * log_page_size)
            .take(log_page_size)
            .cloned()
            .collect::<Vec<_>>()
    };

    Ok(json!({
      "summary": finalize_summary(summary),
      "providerStats": create_group_stats(
        &logs,
        |log| string_value(log.get("providerId")),
        |log| {
          let mut base = Map::new();
          base.insert("providerId".to_string(), log.get("providerId").cloned().unwrap_or(Value::Null));
          base.insert("providerName".to_string(), log.get("providerName").cloned().unwrap_or(Value::Null));
          base.insert("providerType".to_string(), log.get("providerType").cloned().unwrap_or(Value::Null));
          base
        }
      ),
      "modelStats": create_group_stats(
        &logs,
        |log| format!("{}:{}:{}", string_value(log.get("providerId")), string_value(log.get("appType")), non_empty_text(log.get("model"), "unknown")),
        |log| {
          let mut base = Map::new();
          base.insert("providerId".to_string(), log.get("providerId").cloned().unwrap_or(Value::Null));
          base.insert("appType".to_string(), log.get("appType").cloned().unwrap_or(Value::Null));
          base.insert("model".to_string(), json!(non_empty_text(log.get("model"), "未识别模型")));
          base.insert("providerName".to_string(), log.get("providerName").cloned().unwrap_or(Value::Null));
          base
        }
      ),
      "trends": trends,
      "trendSeries": {
        "models": model_trend_series,
        "providers": provider_trend_series
      },
      "logs": response_logs,
      "logTotalCount": log_total_count,
      "filters": {
        "appTypes": usage_store::read_app_types(paths)?,
        "providers": create_provider_filter_options(&option_logs),
        "models": unique_strings(option_logs.iter().map(|item| string_value(item.get("model"))).filter(|item| !item.is_empty()).collect()),
        "requestSources": unique_strings(option_logs.iter().map(|item| non_empty_text(item.get("requestSource"), "session")).collect())
      },
      "pricingConfig": pricing_config
    }))
}

fn get_provider_stats_data(
    paths: &AppPaths,
    pricing_config: &Value,
    pricing_index: &HashMap<String, Value>,
    filters: &Value,
) -> Result<Value, ManagerError> {
    let raw_logs = read_usage_logs(paths)?;
    let today_start_at = today_start_at();
    let cache_key = format!(
        "{}|{}",
        provider_stats_cache_key(filters),
        create_hash_id(&[pricing_config.to_string()])
    );
    let cache = USAGE_PROVIDER_STATS_CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache
        .lock()
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let cached = cache.entry(cache_key).or_insert_with(|| UsageProviderStatsCache {
        path: paths.storage_files.database.clone(),
        logs: Arc::new(Vec::new()),
        today_start_at,
        log_signatures: HashMap::new(),
        summary: create_empty_summary(),
        today_summary: create_empty_summary(),
        model_stats: HashMap::new(),
        today_model_stats: HashMap::new(),
    });

    if cached.path == paths.storage_files.database
        && cached.today_start_at == today_start_at
        && Arc::ptr_eq(&cached.logs, &raw_logs)
    {
        return Ok(provider_stats_cache_response(cached, pricing_config));
    }

    let current_log_signatures = raw_logs
        .iter()
        .map(|log| (usage_log_cache_id(log), usage_log_cache_signature(log)))
        .collect::<HashMap<_, _>>();

    if cached.path != paths.storage_files.database
        || cached.today_start_at != today_start_at
        || cached
            .log_signatures
            .iter()
            .any(|(request_id, signature)| current_log_signatures.get(request_id) != Some(signature))
    {
        cached.path = paths.storage_files.database.clone();
        cached.logs = raw_logs.clone();
        cached.today_start_at = today_start_at;
        cached.log_signatures.clear();
        cached.summary = create_empty_summary();
        cached.today_summary = create_empty_summary();
        cached.model_stats.clear();
        cached.today_model_stats.clear();
    }

    for log in raw_logs.iter() {
        let request_id = usage_log_cache_id(log);

        if cached.log_signatures.contains_key(&request_id) {
            continue;
        }

        cached
            .log_signatures
            .insert(request_id, usage_log_cache_signature(log));

        if !in_range(log, filters) {
            continue;
        }

        let enriched_log = enrich_usage_log(log, pricing_config, pricing_index);

        append_usage_summary(&mut cached.summary, &enriched_log);
        append_usage_model_group(&mut cached.model_stats, &enriched_log);

        if number_value(enriched_log.get("createdAt"), 0) >= today_start_at {
            append_usage_summary(&mut cached.today_summary, &enriched_log);
            append_usage_model_group(&mut cached.today_model_stats, &enriched_log);
        }
    }

    cached.logs = raw_logs.clone();

    Ok(provider_stats_cache_response(cached, pricing_config))
}

fn provider_stats_cache_response(cached: &UsageProviderStatsCache, pricing_config: &Value) -> Value {
    json!({
      "summary": finalize_summary(cached.summary.clone()),
      "todaySummary": finalize_summary(cached.today_summary.clone()),
      "modelStats": finalize_usage_model_groups(&cached.model_stats),
      "todayModelStats": finalize_usage_model_groups(&cached.today_model_stats),
      "logTotalCount": cached.summary.request_count,
      "filters": {
        "appTypes": [],
        "providers": [],
        "models": [],
        "requestSources": []
      },
      "pricingConfig": pricing_config
    })
}

fn get_initial_state_data(paths: &AppPaths) -> Result<Value, ManagerError> {
    let pricing_config = read_pricing(paths)?;
    let pricing_index = create_pricing_index(&pricing_config);
    let raw_logs = read_usage_logs(paths)?;
    let logs = raw_logs
        .iter()
        .take(200)
        .map(|item| enrich_usage_log(item, &pricing_config, &pricing_index))
        .collect::<Vec<_>>();
    let mut summary = create_empty_summary();

    for log in raw_logs.iter() {
        append_priced_usage_summary(&mut summary, log, &pricing_config, &pricing_index);
    }

    Ok(json!({
      "summary": finalize_summary(summary),
      "providerStats": create_group_stats(
        &logs,
        |log| string_value(log.get("providerId")),
        |log| {
          let mut base = Map::new();
          base.insert("providerId".to_string(), log.get("providerId").cloned().unwrap_or(Value::Null));
          base.insert("providerName".to_string(), log.get("providerName").cloned().unwrap_or(Value::Null));
          base.insert("providerType".to_string(), log.get("providerType").cloned().unwrap_or(Value::Null));
          base
        }
      ),
      "modelStats": create_group_stats(
        &logs,
        |log| format!("{}:{}:{}", string_value(log.get("providerId")), string_value(log.get("appType")), non_empty_text(log.get("model"), "unknown")),
        |log| {
          let mut base = Map::new();
          base.insert("providerId".to_string(), log.get("providerId").cloned().unwrap_or(Value::Null));
          base.insert("appType".to_string(), log.get("appType").cloned().unwrap_or(Value::Null));
          base.insert("model".to_string(), json!(non_empty_text(log.get("model"), "未识别模型")));
          base.insert("providerName".to_string(), log.get("providerName").cloned().unwrap_or(Value::Null));
          base
        }
      ),
      "trends": [],
      "trendSeries": {
        "models": [],
        "providers": []
      },
      "logs": logs,
      "logTotalCount": raw_logs.len(),
      "filters": {
        "appTypes": unique_strings(logs.iter().map(|item| string_value(item.get("appType"))).collect()),
        "providers": create_provider_filter_options(&logs),
        "models": unique_strings(logs.iter().map(|item| string_value(item.get("model"))).filter(|item| !item.is_empty()).collect()),
        "requestSources": unique_strings(logs.iter().map(|item| non_empty_text(item.get("requestSource"), "session")).collect())
      },
      "pricingConfig": pricing_config,
      "initialLite": true
    }))
}

async fn refresh_usage(paths: &AppPaths, state: &Value) -> Result<Vec<Value>, ManagerError> {
    let mut diagnostics = Vec::new();
    let mut updates = Vec::new();
    let mut removed_subagent_paths = Vec::new();
    let workspace_created_at = std::fs::metadata(&paths.workspace_root)?
        .created()?
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| ManagerError::System(error.to_string()))?
        .as_millis() as u64;
    let session_versions = usage_store::read_session_versions(paths)?;

    for mut session in collect_usage_sessions(paths, state)? {
        let app_type = normalize_app_type(&string_value(session.get("cli")));
        let raw_path = string_value(session.get("rawPath"));

        if !["claude", "codex", "gemini"].contains(&app_type.as_str()) || raw_path.is_empty() {
            continue;
        }

        let session_updated_at = file_modified_at(&raw_path);
        session["updatedAt"] = json!(session_updated_at);

        if app_type == "codex" && is_codex_subagent_session(&raw_path)? {
            if session_versions.contains_key(&raw_path) {
                removed_subagent_paths.push(raw_path);
            }
            continue;
        }

        if session_versions.get(&raw_path) == Some(&session_updated_at) {
            continue;
        }

        match parse_usage_session(&session, state, workspace_created_at).await {
            Ok(logs) => {
                let request_ids = logs
                    .iter()
                    .map(|log| string_value(log.get("requestId")))
                    .filter(|request_id| !request_id.is_empty())
                    .collect::<Vec<_>>();
                let record_map = usage_store::read_request_records(paths, &request_ids)?;
                let (logs, records) = merge_usage_records(
                    logs,
                    record_map,
                    state,
                    workspace_created_at,
                );

                updates.push(UsageSessionUpdate {
                    raw_path,
                    app_type,
                    updated_at: session_updated_at,
                    logs,
                    records,
                });
            }
            Err(error) => diagnostics.push(json!({
              "type": "usage-parse-error",
              "message": error.to_string(),
              "sourcePath": raw_path
            })),
        }
    }

    usage_store::remove_usage_sessions(paths, &removed_subagent_paths)?;
    usage_store::replace_sessions(paths, &updates)?;
    Ok(diagnostics)
}

async fn parse_usage_session(
    session: &Value,
    state: &Value,
    workspace_created_at: u64,
) -> Result<Vec<Value>, ManagerError> {
    let app_type = normalize_app_type(&string_value(session.get("cli")));
    let raw_path = string_value(session.get("rawPath"));
    let content = tokio::fs::read_to_string(&raw_path).await?;
    let extension = Path::new(&raw_path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mut session_created_at = session_content_created_at(&app_type, &extension, &content)?;

    if session_created_at == 0 {
        session_created_at = std::fs::metadata(&raw_path)?
            .created()?
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| ManagerError::System(error.to_string()))?
            .as_millis() as u64;
    }

    let is_legacy_unbound_session = session_created_at < workspace_created_at
        && string_value(session.get("requestSource")).is_empty()
        && string_value(session.get("instanceProviderId")).is_empty();
    let request_source = if is_legacy_unbound_session {
        String::new()
    } else if !string_value(session.get("requestSource")).is_empty() {
        string_value(session.get("requestSource"))
    } else if proxy_state_enabled(state, &app_type) {
        "proxy-managed".to_string()
    } else {
        String::new()
    };
    let mut usage_session = session.clone();

    usage_session["requestSource"] = json!(request_source);
    usage_session["sessionCreatedAt"] = json!(session_created_at);
    let fallback_provider = if is_legacy_unbound_session {
        json!({
          "providerId": app_type,
          "providerName": format_app_provider_name(&app_type),
          "providerType": ""
        })
    } else {
        resolve_provider(&app_type, state)
    };
    let provider_info = create_session_provider_info(&usage_session, &fallback_provider);

    if app_type == "claude" {
        return extract_claude_logs(&usage_session, &content, &provider_info);
    }

    if app_type == "codex" && extension != "json" {
        return extract_codex_logs(&usage_session, &content, &provider_info);
    }

    if app_type == "gemini" {
        return extract_gemini_logs(&usage_session, &content, &provider_info);
    }

    Ok(Vec::new())
}

fn merge_usage_records(
    logs: Vec<Value>,
    mut record_map: HashMap<String, Value>,
    state: &Value,
    workspace_created_at: u64,
) -> (Vec<Value>, Vec<Value>) {
    let providers = state
        .get("providers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut merged_logs = Vec::new();

    for log in logs {
        let request_id = string_value(log.get("requestId"));
        let record = record_map.get(&request_id).cloned();
        let app_type = string_value(log.get("appType"));
        let log_session_created_at = number_value(
            log.get("sessionCreatedAt"),
            number_value(log.get("createdAt"), 0),
        );
        let is_legacy_unbound_log = log_session_created_at > 0
            && log_session_created_at < workspace_created_at
            && string_value(log.get("requestSource")).is_empty()
            && string_value(log.get("instanceProviderId")).is_empty();
        let should_refresh_proxy_record = is_legacy_unbound_log
            && proxy_state_enabled(state, &app_type)
            && record
                .as_ref()
                .is_some_and(|item| string_value(item.get("providerId")) == app_type);
        let should_refresh_instance_record = is_legacy_unbound_log
            && string_value(log.get("requestSource")) == "provider-instance"
            && record.as_ref().is_some_and(|item| {
                string_value(item.get("providerId")) != string_value(log.get("providerId"))
                    || string_value(item.get("requestSource"))
                        != string_value(log.get("requestSource"))
            });
        let should_refresh_legacy_record = is_legacy_unbound_log
            && record.as_ref().is_some_and(|item| {
                string_value(item.get("providerId")) != app_type
                    || !string_value(item.get("requestSource")).is_empty()
                    || !string_value(item.get("instanceProviderId")).is_empty()
            });
        let provider_info = if let Some(record) = record.as_ref() {
            if !should_refresh_proxy_record
                && !should_refresh_instance_record
                && !should_refresh_legacy_record
            {
                let provider_id = string_value(record.get("providerId"));
                let provider = providers
                    .iter()
                    .find(|item| string_value(item.get("id")) == provider_id);

                json!({
                  "providerId": provider_id,
                  "providerName": provider.map(|item| string_value(item.get("name"))).filter(|item| !item.is_empty()).unwrap_or_else(|| string_value(record.get("providerName"))),
                  "providerType": provider.map(|item| string_value(item.get("type"))).filter(|item| !item.is_empty()).unwrap_or_else(|| string_value(record.get("providerType")))
                })
            } else {
                create_log_provider_info(&log)
            }
        } else {
            create_log_provider_info(&log)
        };
        let request_record = if let Some(record) = record.as_ref() {
            if !should_refresh_proxy_record
                && !should_refresh_instance_record
                && !should_refresh_legacy_record
            {
                let mut log_with_record = log.clone();

                log_with_record["requestSource"] = json!(string_value(record.get("requestSource")));
                log_with_record["instanceProviderId"] =
                    json!(string_value(record.get("instanceProviderId")));
                log_with_record["instanceProviderName"] =
                    json!(string_value(record.get("instanceProviderName")));
                log_with_record["instanceProviderType"] =
                    json!(string_value(record.get("instanceProviderType")));
                create_request_record(&log_with_record, &provider_info)
            } else {
                create_request_record(&log, &provider_info)
            }
        } else {
            create_request_record(&log, &provider_info)
        };

        record_map.insert(request_id, request_record.clone());
        merged_logs.push(apply_request_record(&log, &request_record));
    }

    merged_logs.sort_by(|left, right| {
        number_value(right.get("createdAt"), 0).cmp(&number_value(left.get("createdAt"), 0))
    });
    let mut seen_logs = HashSet::new();

    merged_logs.retain(|log| seen_logs.insert(string_value(log.get("requestId"))));
    let mut records = record_map.into_values().collect::<Vec<_>>();

    records.sort_by(|left, right| {
        number_value(right.get("createdAt"), 0).cmp(&number_value(left.get("createdAt"), 0))
    });
    (merged_logs, records)
}

fn read_pricing(paths: &AppPaths) -> Result<Value, ManagerError> {
    normalize_pricing_config(usage_store::read_pricing(paths)?)
}

async fn write_pricing(paths: &AppPaths, pricing: &Value) -> Result<(), ManagerError> {
    usage_store::write_pricing(paths, pricing)
}

fn normalize_pricing_config(input: Value) -> Result<Value, ManagerError> {
    let exchange_rate = price_number(input.get("exchangeRate"), DEFAULT_EXCHANGE_RATE);

    if exchange_rate <= 0.0 {
        return Err(ManagerError::System("汇率必须大于 0".to_string()));
    }

    let items = input
        .get("items")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(normalize_pricing_item)
        .collect::<Result<Vec<_>, _>>()?;

    Ok(json!({
      "exchangeRate": exchange_rate,
      "items": items
    }))
}

fn normalize_pricing_item(input: Value) -> Result<Value, ManagerError> {
    let model_id = input
        .get("modelId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();

    if model_id.is_empty() {
        return Err(ManagerError::System("模型名称不能为空".to_string()));
    }

    let generated_id = create_pricing_id();

    Ok(json!({
      "id": non_empty_text(input.get("id"), &generated_id),
      "modelId": model_id,
      "modelCategory": normalize_model_category(input.get("modelCategory").or_else(|| input.get("category"))),
      "currency": normalize_currency(input.get("currency")),
      "inputCostPerMillion": price_number(input.get("inputCostPerMillion"), 0.0),
      "outputCostPerMillion": price_number(input.get("outputCostPerMillion"), 0.0),
      "cacheReadCostPerMillion": price_number(input.get("cacheReadCostPerMillion"), 0.0),
      "cacheCreationCostPerMillion": price_number(input.get("cacheCreationCostPerMillion"), 0.0)
    }))
}

fn extract_claude_logs(
    session: &Value,
    content: &str,
    provider_info: &Value,
) -> Result<Vec<Value>, ManagerError> {
    let mut logs = Vec::new();

    for line in content.lines() {
        let text = line.trim();

        if text.is_empty() {
            continue;
        }

        let record: Value = serde_json::from_str(text)?;
        let message = record.get("message").or_else(|| {
            record
                .get("payload")
                .and_then(|payload| payload.get("message"))
        });
        let usage = message.and_then(|message| message.get("usage"));

        if record.get("type").and_then(Value::as_str) != Some("assistant")
            || usage.is_none()
            || message
                .and_then(|message| message.get("stop_reason"))
                .is_none_or(Value::is_null)
            || to_number(usage.and_then(|usage| usage.get("output_tokens"))) <= 0
        {
            continue;
        }

        let message_id = message
            .and_then(|message| message.get("id"))
            .and_then(Value::as_str)
            .or_else(|| record.get("uuid").and_then(Value::as_str))
            .unwrap_or("");
        let request_id = if message_id.is_empty() {
            format!(
                "session:{}",
                create_hash_id(&[
                    string_value(session.get("id")),
                    value_to_text(record.get("timestamp")),
                    logs.len().to_string(),
                ])
            )
        } else {
            format!("session:{}", message_id)
        };

        logs.push(create_usage_log(
            session,
            provider_info,
            &request_id,
            message
                .and_then(|message| message.get("model"))
                .map(|value| value_to_text(Some(value)))
                .unwrap_or_default(),
            "",
            to_number(usage.and_then(|usage| usage.get("input_tokens"))),
            to_number(usage.and_then(|usage| usage.get("output_tokens"))),
            to_number(usage.and_then(|usage| usage.get("cache_read_input_tokens"))),
            first_positive_number(&[
                usage.and_then(|usage| usage.get("cache_creation_input_tokens")),
                usage
                    .and_then(|usage| usage.get("cache_creation"))
                    .and_then(|value| value.get("ephemeral_1h_input_tokens")),
                usage
                    .and_then(|usage| usage.get("cache_creation"))
                    .and_then(|value| value.get("ephemeral_5m_input_tokens")),
            ]),
            "session_log",
            to_timestamp(
                record.get("timestamp"),
                number_value(session.get("updatedAt"), 0),
            ),
        ));
    }

    Ok(logs)
}

fn extract_codex_logs(
    session: &Value,
    content: &str,
    provider_info: &Value,
) -> Result<Vec<Value>, ManagerError> {
    let mut logs = Vec::new();
    let mut model = string_value(session.get("model"));
    let mut previous_total_usage: Option<Map<String, Value>> = None;

    for line in content.lines() {
        let text = line.trim();

        if text.is_empty() {
            continue;
        }

        let record: Value = serde_json::from_str(text)?;
        let payload = record.get("payload").unwrap_or(&record);

        if !string_value(payload.get("model")).is_empty() {
            model = string_value(payload.get("model"));
        }

        if payload.get("type").and_then(Value::as_str) == Some("session_meta") {
            model = first_text(
                payload.get("model"),
                payload.get("metadata").and_then(|value| value.get("model")),
                &model,
            );
            continue;
        }

        if payload.get("type").and_then(Value::as_str) != Some("token_count") {
            continue;
        }

        let info = payload.get("info").unwrap_or(&Value::Null);
        let total_usage = normalize_codex_token_usage(info.get("total_token_usage"));
        let last_usage = normalize_codex_token_usage(info.get("last_token_usage"));
        let delta = subtract_token_usage(&total_usage, previous_total_usage.as_ref());
        let usage = if is_valid_codex_delta(&delta) {
            delta
        } else {
            last_usage
        };

        previous_total_usage = Some(total_usage);

        if !is_valid_codex_delta(&usage) {
            continue;
        }

        logs.push(create_usage_log(
            session,
            provider_info,
            &format!(
                "codex:{}",
                create_hash_id(&[
                    string_value(session.get("id")),
                    value_to_text(record.get("timestamp")),
                    logs.len().to_string(),
                    number_value(usage.get("input_tokens"), 0).to_string(),
                    number_value(usage.get("output_tokens"), 0).to_string(),
                ])
            ),
            model.clone(),
            "",
            number_value(usage.get("input_tokens"), 0),
            number_value(usage.get("output_tokens"), 0),
            number_value(usage.get("cached_input_tokens"), 0),
            0,
            "codex_session",
            to_timestamp(
                record.get("timestamp"),
                number_value(session.get("updatedAt"), 0),
            ),
        ));
    }

    Ok(logs)
}

fn extract_gemini_logs(
    session: &Value,
    content: &str,
    provider_info: &Value,
) -> Result<Vec<Value>, ManagerError> {
    let payload: Value = serde_json::from_str(content)?;
    let mut items = Vec::new();
    let mut logs = Vec::new();

    collect_gemini_usage_items(&payload, &mut items);

    for item in items {
        let usage = item.get("usageMetadata").unwrap_or(&Value::Null);
        let input_tokens = get_usage_value(usage, &["promptTokenCount"]);
        let total_tokens = get_usage_value(usage, &["totalTokenCount"]);
        let output_tokens = total_tokens.saturating_sub(input_tokens);

        if input_tokens + output_tokens <= 0 {
            continue;
        }

        logs.push(create_usage_log(
            session,
            provider_info,
            &format!(
                "gemini:{}",
                create_hash_id(&[
                    string_value(session.get("id")),
                    logs.len().to_string(),
                    input_tokens.to_string(),
                    output_tokens.to_string(),
                    first_text(item.get("createTime"), item.get("timestamp"), ""),
                ])
            ),
            first_text(
                item.get("model"),
                payload.get("model"),
                &string_value(session.get("model")),
            ),
            "",
            input_tokens,
            output_tokens,
            get_usage_value(usage, &["cachedContentTokenCount"]),
            0,
            "gemini_session",
            to_timestamp(
                item.get("createTime")
                    .or_else(|| item.get("timestamp"))
                    .or_else(|| payload.get("updatedAt")),
                number_value(session.get("updatedAt"), 0),
            ),
        ));
    }

    Ok(logs)
}

fn create_usage_log(
    session: &Value,
    provider_info: &Value,
    request_id: &str,
    model: String,
    request_model: &str,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    cache_creation_tokens: u64,
    data_source: &str,
    created_at: u64,
) -> Value {
    let session_model = string_value(session.get("model"));
    let model = if model.is_empty() {
        session_model
    } else {
        model
    };
    let request_model = if request_model.is_empty() {
        model.clone()
    } else {
        request_model.to_string()
    };

    json!({
      "requestId": request_id,
      "providerId": provider_info["providerId"],
      "providerName": provider_info["providerName"],
      "providerType": provider_info["providerType"],
      "appType": string_value(session.get("cli")),
      "model": model,
      "requestModel": request_model,
      "inputTokens": input_tokens,
      "outputTokens": output_tokens,
      "cacheReadTokens": cache_read_tokens,
      "cacheCreationTokens": cache_creation_tokens,
      "inputCostUsd": 0,
      "outputCostUsd": 0,
      "cacheReadCostUsd": 0,
      "cacheCreationCostUsd": 0,
      "totalCostUsd": 0,
      "statusCode": 200,
      "errorMessage": "",
      "sessionId": session.get("id").cloned().unwrap_or(Value::Null),
      "sessionTitle": session.get("title").cloned().unwrap_or(Value::Null),
      "projectName": string_value(session.get("projectName")),
      "rawPath": session.get("rawPath").cloned().unwrap_or(Value::Null),
      "dataSource": data_source,
      "requestSource": string_value(session.get("requestSource")),
      "instanceProviderId": string_value(session.get("instanceProviderId")),
      "instanceProviderName": string_value(session.get("instanceProviderName")),
      "instanceProviderType": string_value(session.get("instanceProviderType")),
      "sessionCreatedAt": number_value(session.get("sessionCreatedAt"), 0),
      "sessionUpdatedAt": number_value(session.get("updatedAt"), 0),
      "createdAt": created_at
    })
}

fn enrich_usage_log(
    log: &Value,
    pricing_config: &Value,
    pricing_index: &HashMap<String, Value>,
) -> Value {
    let mut source_log = log.clone();
    let app_type = string_value(source_log.get("appType"));

    if string_value(source_log.get("providerId")).is_empty() {
        source_log["providerId"] = json!(app_type.clone());
    }

    if string_value(source_log.get("providerName")).is_empty() {
        source_log["providerName"] = json!(format_app_provider_name(&app_type));
    }

    if source_log.get("providerType").is_none() {
        source_log["providerType"] = json!("");
    }

    let costs = calculate_cost_usd(&source_log, pricing_config, pricing_index);

    source_log["actualTokens"] = json!(to_actual_tokens(&source_log));
    source_log["inputCostUsd"] = costs["inputCostUsd"].clone();
    source_log["outputCostUsd"] = costs["outputCostUsd"].clone();
    source_log["cacheReadCostUsd"] = costs["cacheReadCostUsd"].clone();
    source_log["cacheCreationCostUsd"] = costs["cacheCreationCostUsd"].clone();
    source_log["totalCostUsd"] = costs["totalCostUsd"].clone();
    source_log
}

fn calculate_cost_usd(
    log: &Value,
    pricing_config: &Value,
    pricing_index: &HashMap<String, Value>,
) -> Value {
    let pricing = find_model_pricing(log, pricing_index);

    if pricing.is_none() {
        return json!({
          "inputCostUsd": 0,
          "outputCostUsd": 0,
          "cacheReadCostUsd": 0,
          "cacheCreationCostUsd": 0,
          "totalCostUsd": 0
        });
    }

    let pricing = pricing.unwrap();
    let exchange_rate = price_number(pricing_config.get("exchangeRate"), DEFAULT_EXCHANGE_RATE);
    let currency = string_value(pricing.get("currency"));
    let input_cost_usd = (normalize_billable_input(log) as f64
        * price_to_usd(
            price_number(pricing.get("inputCostPerMillion"), 0.0),
            &currency,
            exchange_rate,
        ))
        / 1_000_000.0;
    let output_cost_usd = (to_number(log.get("outputTokens")) as f64
        * price_to_usd(
            price_number(pricing.get("outputCostPerMillion"), 0.0),
            &currency,
            exchange_rate,
        ))
        / 1_000_000.0;
    let cache_read_cost_usd = (to_number(log.get("cacheReadTokens")) as f64
        * price_to_usd(
            price_number(pricing.get("cacheReadCostPerMillion"), 0.0),
            &currency,
            exchange_rate,
        ))
        / 1_000_000.0;
    let cache_creation_cost_usd = (to_number(log.get("cacheCreationTokens")) as f64
        * price_to_usd(
            price_number(pricing.get("cacheCreationCostPerMillion"), 0.0),
            &currency,
            exchange_rate,
        ))
        / 1_000_000.0;

    json!({
      "inputCostUsd": input_cost_usd,
      "outputCostUsd": output_cost_usd,
      "cacheReadCostUsd": cache_read_cost_usd,
      "cacheCreationCostUsd": cache_creation_cost_usd,
      "totalCostUsd": input_cost_usd + output_cost_usd + cache_read_cost_usd + cache_creation_cost_usd
    })
}

fn append_usage_summary(summary: &mut Summary, log: &Value) {
    summary.request_count += 1;
    summary.input_tokens += normalize_billable_input(log);
    summary.output_tokens += to_number(log.get("outputTokens"));
    summary.cache_read_tokens += to_number(log.get("cacheReadTokens"));
    summary.cache_creation_tokens += to_number(log.get("cacheCreationTokens"));
    summary.actual_tokens += to_actual_tokens(log);
    summary.total_cost_usd += price_number(log.get("totalCostUsd"), 0.0);
    summary.last_used_at = summary
        .last_used_at
        .max(number_value(log.get("createdAt"), 0));
}

fn append_priced_usage_summary(
    summary: &mut Summary,
    log: &Value,
    pricing_config: &Value,
    pricing_index: &HashMap<String, Value>,
) {
    summary.request_count += 1;
    summary.input_tokens += normalize_billable_input(log);
    summary.output_tokens += to_number(log.get("outputTokens"));
    summary.cache_read_tokens += to_number(log.get("cacheReadTokens"));
    summary.cache_creation_tokens += to_number(log.get("cacheCreationTokens"));
    summary.actual_tokens += to_actual_tokens(log);
    summary.total_cost_usd += price_number(
        calculate_cost_usd(log, pricing_config, pricing_index).get("totalCostUsd"),
        0.0,
    );
    summary.last_used_at = summary
        .last_used_at
        .max(number_value(log.get("createdAt"), 0));
}

fn append_skill_log_summary(summary: &mut Summary, log: &Value) {
    summary.request_count += 1;
    summary.input_tokens += normalize_billable_input(log);
    summary.output_tokens += to_number(log.get("outputTokens"));
    summary.cache_read_tokens += to_number(log.get("cacheReadTokens"));
    summary.cache_creation_tokens += to_number(log.get("cacheCreationTokens"));
    summary.actual_tokens += if number_value(log.get("actualTokens"), 0) > 0 {
        number_value(log.get("actualTokens"), 0)
    } else {
        to_actual_tokens(log)
    };
    summary.total_cost_usd += price_number(log.get("totalCostUsd"), 0.0);
    summary.last_used_at = summary
        .last_used_at
        .max(number_value(log.get("createdAt"), 0));
}

fn finalize_summary(summary: Summary) -> Value {
    let cache_base =
        summary.input_tokens + summary.cache_read_tokens + summary.cache_creation_tokens;
    let cache_hit_rate = if cache_base > 0 {
        round_to(summary.cache_read_tokens as f64 / cache_base as f64, 4)
    } else {
        0.0
    };

    json!({
      "requestCount": summary.request_count,
      "inputTokens": summary.input_tokens,
      "outputTokens": summary.output_tokens,
      "cacheReadTokens": summary.cache_read_tokens,
      "cacheCreationTokens": summary.cache_creation_tokens,
      "actualTokens": summary.actual_tokens,
      "cacheHitRate": cache_hit_rate,
      "totalCostUsd": round_to(summary.total_cost_usd, 8),
      "lastUsedAt": summary.last_used_at
    })
}

fn finalize_skill_summary(mut summary: Summary) -> Value {
    let mut value = finalize_summary(summary.clone());

    value["usageCount"] = json!(summary.usage_count);
    summary.total_cost_usd = round_to(summary.total_cost_usd, 8);
    value["totalCostUsd"] = json!(summary.total_cost_usd);
    value
}

fn create_group_stats(
    logs: &[Value],
    key_selector: impl Fn(&Value) -> String,
    base_selector: impl Fn(&Value) -> Map<String, Value>,
) -> Vec<Value> {
    let mut groups: HashMap<String, GroupStat> = HashMap::new();

    for log in logs {
        let key = non_empty_owned(key_selector(log), "unknown");

        groups.entry(key).or_insert_with(|| GroupStat {
            base: base_selector(log),
            summary: create_empty_summary(),
        });

        if let Some(group) = groups.get_mut(&non_empty_owned(key_selector(log), "unknown")) {
            append_usage_summary(&mut group.summary, log);
        }
    }

    let mut items = groups
        .into_values()
        .map(|group| merge_summary(group.base, finalize_summary(group.summary)))
        .collect::<Vec<_>>();

    items.sort_by(|left, right| {
        number_value(right.get("actualTokens"), 0).cmp(&number_value(left.get("actualTokens"), 0))
    });
    items
}

fn append_usage_model_group(groups: &mut HashMap<String, GroupStat>, log: &Value) {
    let key = format!(
        "{}:{}:{}",
        string_value(log.get("providerId")),
        string_value(log.get("appType")),
        non_empty_text(log.get("model"), "unknown")
    );

    groups.entry(key.clone()).or_insert_with(|| {
        let mut base = Map::new();

        base.insert(
            "providerId".to_string(),
            log.get("providerId").cloned().unwrap_or(Value::Null),
        );
        base.insert(
            "appType".to_string(),
            log.get("appType").cloned().unwrap_or(Value::Null),
        );
        base.insert(
            "model".to_string(),
            json!(non_empty_text(log.get("model"), "未识别模型")),
        );
        base.insert(
            "providerName".to_string(),
            log.get("providerName").cloned().unwrap_or(Value::Null),
        );

        GroupStat {
            base,
            summary: create_empty_summary(),
        }
    });

    if let Some(group) = groups.get_mut(&key) {
        append_usage_summary(&mut group.summary, log);
    }
}

fn finalize_usage_model_groups(groups: &HashMap<String, GroupStat>) -> Vec<Value> {
    let mut items = groups
        .values()
        .cloned()
        .map(|group| merge_summary(group.base, finalize_summary(group.summary)))
        .collect::<Vec<_>>();

    items.sort_by(|left, right| {
        number_value(right.get("actualTokens"), 0).cmp(&number_value(left.get("actualTokens"), 0))
    });
    items
}

fn create_provider_filter_options(logs: &[Value]) -> Vec<Value> {
    let mut items = Vec::new();
    let mut seen = HashMap::new();

    for log in logs {
        let app_type = string_value(log.get("appType"));
        let provider_id = non_empty_text(log.get("providerId"), &app_type);

        if seen.contains_key(&provider_id) {
            continue;
        }

        seen.insert(provider_id.clone(), true);
        items.push(json!({
          "providerId": provider_id,
          "providerName": non_empty_text(log.get("providerName"), &format_app_provider_name(&app_type)),
          "providerType": string_value(log.get("providerType"))
        }));
    }

    items
}

fn create_usage_trend_stats(logs: &[Value], trend_mode: &str, filters: &Value) -> Vec<Value> {
    let mut groups: HashMap<String, GroupStat> = HashMap::new();
    let is_single_day = is_single_day(filters);

    if trend_mode == "hour" && is_single_day {
        for hour in 0..24 {
            let label = format!("{:02}:00", hour);
            let mut base = Map::new();

            base.insert("date".to_string(), json!(label.clone()));
            base.insert("sortAt".to_string(), json!(hour));
            groups.insert(
                label,
                GroupStat {
                    base,
                    summary: create_empty_summary(),
                },
            );
        }
    }

    if trend_mode == "minute" && is_single_day {
        for hour in 0..24 {
            for minute in 0..60 {
                let label = format!("{:02}:{:02}", hour, minute);
                let mut base = Map::new();

                base.insert("date".to_string(), json!(label.clone()));
                base.insert("sortAt".to_string(), json!(hour * 60 + minute));
                groups.insert(
                    label,
                    GroupStat {
                        base,
                        summary: create_empty_summary(),
                    },
                );
            }
        }
    }

    for log in logs {
        let created_at = number_value(log.get("createdAt"), 0);
        let (label, sort_at) = create_usage_trend_label(created_at, trend_mode, is_single_day);

        groups.entry(label.clone()).or_insert_with(|| {
            let mut base = Map::new();

            base.insert("date".to_string(), json!(label.clone()));
            base.insert("sortAt".to_string(), json!(sort_at));
            GroupStat {
                base,
                summary: create_empty_summary(),
            }
        });

        if let Some(group) = groups.get_mut(&label) {
            append_usage_summary(&mut group.summary, log);
        }
    }

    let mut items = groups
        .into_values()
        .map(|group| {
            let sort_at = group.base.get("sortAt").cloned().unwrap_or(json!(0));
            let mut value = merge_summary(group.base, finalize_summary(group.summary));

            value["sortAt"] = sort_at;
            value
        })
        .collect::<Vec<_>>();

    items.sort_by(|left, right| {
        number_value(left.get("sortAt"), 0).cmp(&number_value(right.get("sortAt"), 0))
    });

    items
        .into_iter()
        .map(|mut item| {
            item.as_object_mut().map(|map| {
                map.remove("sortAt");
            });
            item
        })
        .collect()
}

fn create_usage_model_trend_series(
    logs: &[Value],
    trend_mode: &str,
    filters: &Value,
    trends: &[Value],
) -> Vec<Value> {
    create_usage_group_trend_series(
        logs,
        trend_mode,
        filters,
        trends,
        |log| {
            format!(
                "{}:{}",
                string_value(log.get("appType")),
                non_empty_text(log.get("model"), "未识别模型")
            )
        },
        |log| {
            format!(
                "{} · {}",
                non_empty_text(log.get("model"), "未识别模型"),
                format_app_provider_name(&string_value(log.get("appType")))
            )
        },
    )
}

fn create_usage_provider_trend_series(
    logs: &[Value],
    trend_mode: &str,
    filters: &Value,
    trends: &[Value],
) -> Vec<Value> {
    create_usage_group_trend_series(
        logs,
        trend_mode,
        filters,
        trends,
        |log| string_value(log.get("providerId")),
        |log| string_value(log.get("providerName")),
    )
}

fn create_usage_group_trend_series(
    logs: &[Value],
    trend_mode: &str,
    filters: &Value,
    trends: &[Value],
    key_selector: impl Fn(&Value) -> String,
    name_selector: impl Fn(&Value) -> String,
) -> Vec<Value> {
    let labels = trends
        .iter()
        .map(|item| string_value(item.get("date")))
        .collect::<Vec<_>>();
    let label_index = labels
        .iter()
        .enumerate()
        .map(|(index, label)| (label.clone(), index))
        .collect::<HashMap<_, _>>();
    let is_single_day = is_single_day(filters);
    let mut groups: HashMap<String, Value> = HashMap::new();

    for log in logs {
        let key = non_empty_owned(key_selector(log), "unknown");

        groups.entry(key.clone()).or_insert_with(|| {
            json!({
              "name": name_selector(log),
              "data": labels.iter().map(|_| 0).collect::<Vec<u64>>()
            })
        });

        let (label, _) = create_usage_trend_label(
            number_value(log.get("createdAt"), 0),
            trend_mode,
            is_single_day,
        );
        let Some(index) = label_index.get(&label).cloned() else {
            continue;
        };
        let Some(data) = groups
            .get_mut(&key)
            .and_then(|item| item.get_mut("data"))
            .and_then(Value::as_array_mut)
        else {
            continue;
        };
        let value = data.get(index).and_then(Value::as_u64).unwrap_or(0) + to_actual_tokens(log);

        data[index] = json!(value);
    }

    let mut items = groups.into_values().collect::<Vec<_>>();

    items.retain(|item| {
        item.get("data")
            .and_then(Value::as_array)
            .is_some_and(|data| data.iter().any(|value| value.as_u64().unwrap_or(0) > 0))
    });
    items.sort_by(|left, right| {
        let left_total = left
            .get("data")
            .and_then(Value::as_array)
            .map(|data| {
                data.iter()
                    .map(|value| value.as_u64().unwrap_or(0))
                    .sum::<u64>()
            })
            .unwrap_or(0);
        let right_total = right
            .get("data")
            .and_then(Value::as_array)
            .map(|data| {
                data.iter()
                    .map(|value| value.as_u64().unwrap_or(0))
                    .sum::<u64>()
            })
            .unwrap_or(0);

        right_total.cmp(&left_total)
    });
    items
}

fn create_usage_trend_label(
    created_at: u64,
    trend_mode: &str,
    is_single_day: bool,
) -> (String, u64) {
    let date = local_datetime(created_at);
    let day = format_zh_cn_date(created_at);

    if trend_mode == "minute" {
        let label = if is_single_day {
            format!("{:02}:{:02}", date.hour(), date.minute())
        } else {
            format!("{} {:02}:{:02}", day, date.hour(), date.minute())
        };

        return (label, local_sort_at(created_at, true, true));
    }

    if trend_mode == "hour" {
        let label = if is_single_day {
            format!("{:02}:00", date.hour())
        } else {
            format!("{} {:02}:00", day, date.hour())
        };

        return (label, local_sort_at(created_at, true, false));
    }

    (day, local_sort_at(created_at, false, false))
}

fn create_usage_log_query(filters: &Value) -> UsageLogQuery {
    UsageLogQuery {
        app_type: non_empty_text(filters.get("appType"), "all"),
        provider_id: non_empty_text(filters.get("providerId"), "all"),
        provider_ids: string_array(filters.get("providerIds")),
        model: non_empty_text(filters.get("model"), "all"),
        request_source: non_empty_text(filters.get("requestSource"), "all"),
        start_at: number_value(filters.get("startAt"), 0),
        end_at: number_value(filters.get("endAt"), 0),
    }
}

fn in_range(log: &Value, filters: &Value) -> bool {
    let app_type = string_value(filters.get("appType"));
    let provider_id = string_value(filters.get("providerId"));
    let provider_ids = filters
        .get("providerIds")
        .and_then(Value::as_array)
        .filter(|items| !items.is_empty());
    let model = string_value(filters.get("model"));
    let request_source = string_value(filters.get("requestSource"));
    let start_at = number_value(filters.get("startAt"), 0);
    let end_at = number_value(filters.get("endAt"), 0);

    if app_type != "all" && string_value(log.get("appType")) != app_type {
        return false;
    }

    let log_provider_id = string_value(log.get("providerId"));

    if let Some(provider_ids) = provider_ids {
        if !provider_ids
            .iter()
            .any(|item| string_value(Some(item)) == log_provider_id)
        {
            return false;
        }
    } else if provider_id != "all" && log_provider_id != provider_id {
        return false;
    }

    if model != "all" && string_value(log.get("model")) != model {
        return false;
    }

    if request_source != "all"
        && non_empty_text(log.get("requestSource"), "session") != request_source
    {
        return false;
    }

    if start_at > 0 && number_value(log.get("createdAt"), 0) < start_at {
        return false;
    }

    if end_at > 0 && number_value(log.get("createdAt"), 0) > end_at {
        return false;
    }

    true
}

fn create_request_record(log: &Value, provider_info: &Value) -> Value {
    json!({
      "requestId": log.get("requestId").cloned().unwrap_or(Value::Null),
      "providerId": provider_info["providerId"],
      "providerName": provider_info["providerName"],
      "providerType": string_value(provider_info.get("providerType")),
      "appType": log.get("appType").cloned().unwrap_or(Value::Null),
      "model": string_value(log.get("model")),
      "requestModel": non_empty_text(log.get("requestModel"), &string_value(log.get("model"))),
      "inputTokens": to_number(log.get("inputTokens")),
      "outputTokens": to_number(log.get("outputTokens")),
      "cacheReadTokens": to_number(log.get("cacheReadTokens")),
      "cacheCreationTokens": to_number(log.get("cacheCreationTokens")),
      "actualTokens": to_actual_tokens(log),
      "dataSource": log.get("dataSource").cloned().unwrap_or(Value::Null),
      "sessionId": log.get("sessionId").cloned().unwrap_or(Value::Null),
      "sessionTitle": string_value(log.get("sessionTitle")),
      "projectName": string_value(log.get("projectName")),
      "rawPath": string_value(log.get("rawPath")),
      "requestSource": string_value(log.get("requestSource")),
      "instanceProviderId": string_value(log.get("instanceProviderId")),
      "instanceProviderName": string_value(log.get("instanceProviderName")),
      "instanceProviderType": string_value(log.get("instanceProviderType")),
      "sessionCreatedAt": number_value(log.get("sessionCreatedAt"), 0),
      "sessionUpdatedAt": number_value(log.get("sessionUpdatedAt"), 0),
      "requestTime": log.get("createdAt").cloned().unwrap_or(Value::Null),
      "createdAt": log.get("createdAt").cloned().unwrap_or(Value::Null)
    })
}

fn apply_request_record(log: &Value, record: &Value) -> Value {
    let mut next = log.clone();

    next["providerId"] = record.get("providerId").cloned().unwrap_or(Value::Null);
    next["providerName"] = record.get("providerName").cloned().unwrap_or(Value::Null);
    next["providerType"] = json!(string_value(record.get("providerType")));
    next["requestSource"] = json!(string_value(record.get("requestSource")));
    next["instanceProviderId"] = json!(string_value(record.get("instanceProviderId")));
    next["instanceProviderName"] = json!(string_value(record.get("instanceProviderName")));
    next["instanceProviderType"] = json!(string_value(record.get("instanceProviderType")));
    next["sessionCreatedAt"] = record
        .get("sessionCreatedAt")
        .cloned()
        .unwrap_or(Value::Null);
    next["sessionUpdatedAt"] = record
        .get("sessionUpdatedAt")
        .cloned()
        .unwrap_or(Value::Null);
    next
}

fn create_log_provider_info(log: &Value) -> Value {
    let app_type = string_value(log.get("appType"));

    json!({
      "providerId": non_empty_text(log.get("providerId"), &app_type),
      "providerName": non_empty_text(log.get("providerName"), &format_app_provider_name(&app_type)),
      "providerType": string_value(log.get("providerType"))
    })
}

fn create_session_provider_info(session: &Value, fallback: &Value) -> Value {
    if string_value(session.get("requestSource")) == "provider-instance" {
        return json!({
          "providerId": string_value(session.get("instanceProviderId")),
          "providerName": string_value(session.get("instanceProviderName")),
          "providerType": non_empty_text(session.get("instanceProviderType"), &string_value(fallback.get("providerType")))
        });
    }

    fallback.clone()
}

fn resolve_provider(cli: &str, state: &Value) -> Value {
    let proxy_state = state.get(&format!("{}ProxyState", cli)).or_else(|| {
        if cli == "codex" {
            state.get("codexProxyState")
        } else {
            None
        }
    });
    let proxy_target_id = if proxy_state
        .and_then(|item| item.get("enabled"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        string_value(proxy_state.and_then(|item| item.get("activeProviderId")))
    } else {
        String::new()
    };
    let proxy_account_id = proxy_target_id
        .strip_prefix("account:")
        .unwrap_or("")
        .to_string();
    let codex_accounts = state
        .get("codexAccounts")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let providers = state
        .get("providers")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let proxy_account = codex_accounts
        .iter()
        .find(|item| string_value(item.get("id")) == proxy_account_id);
    let proxy_provider = providers
        .iter()
        .find(|item| string_value(item.get("id")) == proxy_target_id);

    if cli == "codex" {
        if let Some(proxy_account) = proxy_account {
            return json!({
              "providerId": proxy_target_id,
              "providerName": first_text(proxy_account.get("email"), proxy_account.get("accountId"), "Codex 官方账号"),
              "providerType": "codex"
            });
        }
    }

    if let Some(proxy_provider) = proxy_provider {
        if string_value(proxy_provider.get("cli")) == cli {
            return json!({
              "providerId": proxy_provider["id"],
              "providerName": proxy_provider["name"],
              "providerType": proxy_provider["type"]
            });
        }
    }

    let active_codex_account = codex_accounts
        .iter()
        .find(|item| item.get("active").and_then(Value::as_bool) == Some(true));

    if cli == "codex" {
        if let Some(account) = active_codex_account {
            return json!({
              "providerId": format!("codex-account:{}", string_value(account.get("id"))),
              "providerName": first_text(account.get("email"), account.get("accountId"), "Codex 官方账号"),
              "providerType": "codex"
            });
        }
    }

    let runtime_profiles = state
        .get("runtimeProfiles")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let profile = runtime_profiles
        .iter()
        .find(|item| string_value(item.get("cli")) == cli);
    let provider_id = profile
        .map(|item| string_value(item.get("providerId")))
        .filter(|item| !item.is_empty())
        .or_else(|| {
            state
                .get("runtimeProviderState")
                .and_then(|item| item.get(cli))
                .map(|item| string_value(item.get("activeProviderId")))
        })
        .unwrap_or_default();
    let provider = providers
        .iter()
        .find(|item| string_value(item.get("id")) == provider_id);

    if let Some(provider) = provider {
        if string_value(provider.get("cli")) == cli
            && provider.get("enabled").and_then(Value::as_bool) != Some(false)
        {
            return json!({
              "providerId": provider["id"],
              "providerName": provider["name"],
              "providerType": provider["type"]
            });
        }
    }

    json!({
      "providerId": cli,
      "providerName": format_app_provider_name(cli),
      "providerType": ""
    })
}

async fn collect_skills(
    cli_targets: &[Value],
    managed_skills: &[Value],
) -> Result<Vec<SkillInfo>, ManagerError> {
    let mut skill_map: HashMap<String, SkillInfo> = HashMap::new();

    for skill in managed_skills {
        append_skill(
            &mut skill_map,
            &json!({
              "name": skill.get("name").cloned().unwrap_or(Value::Null),
              "description": string_value(skill.get("description")),
              "sourcePath": string_value(skill.get("sourcePath")),
              "cli": "",
              "cliName": ""
            }),
        );
    }

    for cli_target in cli_targets {
        let skills_path = string_value(cli_target.get("skillsPath"));

        if skills_path.is_empty() || !Path::new(&skills_path).exists() {
            continue;
        }

        let cli = get_cli_type(cli_target);
        let skill_roots = scan_skill_roots(&skills_path)?;

        for skill_root in skill_roots {
            let name = read_skill_name(&skill_root).await?;

            append_skill(
                &mut skill_map,
                &json!({
                  "name": name,
                  "description": "",
                  "sourcePath": skill_root,
                  "cli": cli,
                  "cliName": non_empty_text(cli_target.get("name"), &cli)
                }),
            );
        }
    }

    let mut items = skill_map.into_values().collect::<Vec<_>>();

    items.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(items)
}

fn append_skill(skill_map: &mut HashMap<String, SkillInfo>, skill: &Value) {
    let name = string_value(skill.get("name"));

    if name.is_empty() {
        return;
    }

    skill_map.entry(name.clone()).or_insert_with(|| SkillInfo {
        name: name.clone(),
        description: String::new(),
        source_paths: Vec::new(),
        cli_types: Vec::new(),
        aliases: vec![name.clone()],
    });

    let item = skill_map.get_mut(&name).unwrap();
    let description = string_value(skill.get("description"));
    let source_path = string_value(skill.get("sourcePath"));
    let cli = string_value(skill.get("cli"));

    if !description.is_empty() && item.description.is_empty() {
        item.description = description;
    }

    if !source_path.is_empty() && !item.source_paths.contains(&source_path) {
        item.source_paths.push(source_path.clone());
        if let Some(name) = Path::new(&source_path)
            .file_name()
            .and_then(|value| value.to_str())
        {
            item.aliases.push(name.to_string());
        }
    }

    if !cli.is_empty()
        && !item
            .cli_types
            .iter()
            .any(|item| string_value(item.get("id")) == cli)
    {
        item.cli_types.push(json!({
          "id": cli,
          "name": non_empty_text(skill.get("cliName"), &cli)
        }));
    }
}

fn create_alias_map(skills: &[SkillInfo]) -> HashMap<String, String> {
    let mut alias_map = HashMap::new();

    for skill in skills {
        for alias in &skill.aliases {
            let value = alias.trim();

            if !value.is_empty() {
                alias_map.insert(value.to_lowercase(), skill.name.clone());
            }
        }
    }

    alias_map
}

pub(crate) fn collect_cli_session_files(
    cli_targets: &[Value],
) -> Result<Vec<Value>, ManagerError> {
    let mut files = Vec::new();

    for cli_target in cli_targets {
        let cli = get_cli_type(cli_target);
        let cli_name = non_empty_text(cli_target.get("name"), &cli);

        for session_path in get_session_paths(cli_target) {
            if !Path::new(&session_path).exists() {
                continue;
            }

            for file_path in scan_session_files(&session_path, cli_target, 0)? {
                files.push(json!({
                  "cli": cli,
                  "cliName": cli_name,
                  "filePath": file_path,
                  "sourceType": "session"
                }));
            }
        }
    }

    Ok(files)
}

fn collect_usage_sessions(paths: &AppPaths, state: &Value) -> Result<Vec<Value>, ManagerError> {
    let mut sessions = state
        .get("sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let scan_start_at = usage_session_scan_start_at(paths);
    let mut seen_paths = sessions
        .iter()
        .map(|item| string_value(item.get("rawPath")))
        .filter(|item| !item.is_empty())
        .map(|item| (item, true))
        .collect::<HashMap<_, _>>();
    let cli_targets = state
        .get("cliTargets")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    for item in collect_cli_session_files(&cli_targets)? {
        let raw_path = string_value(item.get("filePath"));

        if raw_path.is_empty() || seen_paths.contains_key(&raw_path) {
            continue;
        }

        if scan_start_at > 0 && file_modified_at(&raw_path) < scan_start_at {
            continue;
        }

        seen_paths.insert(raw_path.clone(), true);
        sessions.push(create_scanned_usage_session(&item, &raw_path));
    }

    Ok(sessions)
}

fn usage_session_scan_start_at(paths: &AppPaths) -> u64 {
    let cache_updated_at = file_modified_at(&paths.storage_files.sessions);

    if cache_updated_at == 0 {
        return 0;
    }

    local_sort_at(cache_updated_at, false, false)
}

fn create_scanned_usage_session(item: &Value, raw_path: &str) -> Value {
    let updated_at = file_modified_at(raw_path);
    let title = Path::new(raw_path)
        .file_stem()
        .and_then(|item| item.to_str())
        .unwrap_or(raw_path)
        .to_string();
    let cli = string_value(item.get("cli"));

    json!({
      "id": create_hash_id(&[
        cli.clone(),
        raw_path.to_string()
      ]),
      "cli": cli,
      "cliName": non_empty_text(item.get("cliName"), &string_value(item.get("cli"))),
      "title": title,
      "summary": "",
      "projectName": "",
      "projectPath": "",
      "model": "",
      "rawPath": raw_path,
      "updatedAt": updated_at,
      "archived": false
    })
}

fn is_codex_subagent_session(raw_path: &str) -> Result<bool, ManagerError> {
    let file = std::fs::File::open(raw_path)?;
    let reader = BufReader::new(file);

    for line in reader.lines().take(8) {
        let record: Value = serde_json::from_str(&line?)?;

        if record.get("type").and_then(Value::as_str) != Some("session_meta") {
            continue;
        }

        let payload = record.get("payload").unwrap_or(&record);
        return Ok(string_value(payload.get("thread_source")) == "subagent"
            || payload
                .get("source")
                .and_then(|source| source.get("subagent"))
                .is_some());
    }

    Ok(false)
}

fn file_modified_at(path: &str) -> u64 {
    std::fs::metadata(path)
        .ok()
        .and_then(|item| item.modified().ok())
        .and_then(|item| item.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|item| item.as_millis() as u64)
        .unwrap_or(0)
}

fn read_session_records(item: &Value) -> Result<Vec<Value>, ManagerError> {
    let content = std::fs::read_to_string(string_value(item.get("filePath")))?;
    let mut records = Vec::new();

    for line in content.lines() {
        let text = line.trim();

        if text.is_empty() {
            continue;
        }

        let record: Value = serde_json::from_str(text)?;
        let mut texts = Vec::new();

        collect_session_record_texts(&record, &mut texts);
        records.push(json!({
          "display": texts.join("\n"),
          "timestamp": first_defined_value(&[
            record.get("timestamp"),
            record.get("createdAt"),
            record.get("created_at"),
            record.get("payload").and_then(|value| value.get("timestamp")),
            record.get("message").and_then(|value| value.get("timestamp"))
          ]),
          "rawPath": string_value(item.get("filePath"))
        }));
    }

    Ok(records)
}

fn collect_session_record_texts(record: &Value, output: &mut Vec<String>) {
    let payload = record.get("payload").unwrap_or(record);
    let message = record
        .get("message")
        .or_else(|| payload.get("message"))
        .unwrap_or(payload);
    let role = get_session_record_role(record);

    let tool_arguments = match payload.get("type").and_then(Value::as_str) {
        Some("function_call") => payload.get("arguments"),
        Some("custom_tool_call") => payload.get("input"),
        _ => None,
    };

    if let Some(arguments) = tool_arguments {
        if let Some(text) = arguments.as_str() {
            if let Ok(value) = serde_json::from_str::<Value>(text) {
                collect_text_values(&value, output);
            } else {
                collect_text_values(arguments, output);
            }
        } else {
            collect_text_values(arguments, output);
        }
        return;
    }

    if role == "user" {
        collect_text_values(
            message
                .get("content")
                .or_else(|| payload.get("content"))
                .or_else(|| record.get("display"))
                .unwrap_or(&Value::Null),
            output,
        );
        return;
    }

    if role == "assistant" {
        collect_tool_use_texts(
            message
                .get("content")
                .or_else(|| payload.get("content"))
                .unwrap_or(&Value::Null),
            output,
        );
    }
}

fn collect_text_values(value: &Value, output: &mut Vec<String>) {
    match value {
        Value::String(text) => output.push(text.clone()),
        Value::Array(items) => {
            for item in items {
                collect_text_values(item, output);
            }
        }
        Value::Object(map) => {
            for item in map.values() {
                collect_text_values(item, output);
            }
        }
        _ => {}
    }
}

fn collect_tool_use_texts(content: &Value, output: &mut Vec<String>) {
    let Some(items) = content.as_array() else {
        return;
    };

    for item in items {
        if item.get("type").and_then(Value::as_str) == Some("tool_use") {
            collect_text_values(item.get("input").unwrap_or(&Value::Null), output);
        }
    }
}

fn extract_skill_names(display: &str, alias_map: &HashMap<String, String>) -> Vec<String> {
    let mut matches = Vec::new();

    for capture in slash_skill_regex().captures_iter(display) {
        if let Some(skill_name) = alias_map.get(&capture[1].to_lowercase()) {
            if !matches.contains(skill_name) {
                matches.push(skill_name.clone());
            }
        }
    }

    for capture in path_skill_regex().captures_iter(display) {
        if let Some(skill_name) = alias_map.get(&capture[1].to_lowercase()) {
            if !matches.contains(skill_name) {
                matches.push(skill_name.clone());
            }
        }
    }

    matches
}

fn match_invocation_logs(
    invocations: &[(usize, SkillInvocation)],
    usage_logs: &[Value],
) -> HashMap<usize, Vec<Value>> {
    let mut logs_by_path: HashMap<String, Vec<Value>> = HashMap::new();

    for log in usage_logs {
        let raw_path = string_value(log.get("rawPath"));

        if raw_path.is_empty() {
            continue;
        }

        logs_by_path.entry(raw_path).or_default().push(log.clone());
    }

    for logs in logs_by_path.values_mut() {
        logs.sort_by(|left, right| {
            number_value(left.get("createdAt"), 0).cmp(&number_value(right.get("createdAt"), 0))
        });
    }

    let mut invocations_by_path: HashMap<String, Vec<(usize, SkillInvocation)>> = HashMap::new();

    for (index, invocation) in invocations {
        if invocation.raw_path.is_empty() {
            continue;
        }

        invocations_by_path
            .entry(invocation.raw_path.clone())
            .or_default()
            .push((*index, invocation.clone()));
    }

    let mut result = HashMap::new();

    for (raw_path, mut items) in invocations_by_path {
        let logs = logs_by_path.remove(&raw_path).unwrap_or_default();

        items.sort_by(|left, right| left.1.created_at.cmp(&right.1.created_at));

        for (position, (index, invocation)) in items.iter().enumerate() {
            if invocation.created_at == 0 {
                result.insert(*index, Vec::new());
                continue;
            }

            let next_invocation = items
                .iter()
                .skip(position + 1)
                .find(|(_, item)| item.created_at > invocation.created_at)
                .map(|(_, item)| item.created_at);
            let matched = logs
                .iter()
                .filter(|log| {
                    let created_at = number_value(log.get("createdAt"), 0);

                    if created_at < invocation.created_at {
                        return false;
                    }

                    next_invocation.is_none_or(|next_at| created_at < next_at)
                })
                .cloned()
                .collect::<Vec<_>>();

            result.insert(*index, matched);
        }
    }

    result
}

fn create_skill_rows(
    skills: &[SkillInfo],
    invocations: &[SkillInvocation],
    matched_logs: &HashMap<usize, Vec<Value>>,
    filters: &Value,
) -> Vec<Value> {
    let mut rows = Vec::new();

    for skill in skills {
        let skill_invocations = invocations
            .iter()
            .enumerate()
            .filter(|(_, item)| {
                item.skill_name == skill.name && match_invocation_filters(item, filters)
            })
            .collect::<Vec<_>>();
        let mut logs = Vec::new();
        let mut seen_logs = HashMap::new();

        for (index, _) in &skill_invocations {
            for log in matched_logs.get(index).cloned().unwrap_or_default() {
                let request_id = string_value(log.get("requestId"));

                if !seen_logs.contains_key(&request_id) {
                    seen_logs.insert(request_id, true);
                    logs.push(log);
                }
            }
        }

        let mut summary = create_empty_summary();

        for log in &logs {
            append_skill_log_summary(&mut summary, log);
        }

        summary.usage_count = skill_invocations.len() as u64;
        summary.last_used_at = skill_invocations
            .iter()
            .map(|(_, item)| item.created_at)
            .max()
            .unwrap_or(0);

        let mut row = Map::new();
        row.insert("name".to_string(), json!(skill.name));
        row.insert("description".to_string(), json!(skill.description));
        row.insert("sourcePaths".to_string(), json!(skill.source_paths));
        row.insert("cliTypes".to_string(), json!(skill.cli_types));
        row.insert("aliases".to_string(), json!(skill.aliases));
        row.extend(
            finalize_skill_summary(summary)
                .as_object()
                .cloned()
                .unwrap_or_default(),
        );
        row.insert(
            "providers".to_string(),
            json!(create_group_stats(
                &logs,
                |log| string_value(log.get("providerId")),
                |log| {
                    let mut base = Map::new();
                    let provider_id = string_value(log.get("providerId"));
                    base.insert("providerId".to_string(), json!(provider_id.clone()));
                    base.insert(
                        "providerName".to_string(),
                        json!(non_empty_text(
                            log.get("providerName"),
                            &non_empty_owned(provider_id, "未知 Provider")
                        )),
                    );
                    base
                }
            )),
        );
        row.insert(
            "models".to_string(),
            json!(create_group_stats(
                &logs,
                |log| format!(
                    "{}:{}",
                    string_value(log.get("appType")),
                    non_empty_text(log.get("model"), "unknown")
                ),
                |log| {
                    let mut base = Map::new();
                    base.insert(
                        "appType".to_string(),
                        log.get("appType").cloned().unwrap_or(Value::Null),
                    );
                    base.insert(
                        "model".to_string(),
                        json!(non_empty_text(log.get("model"), "未识别模型")),
                    );
                    base.insert(
                        "providerName".to_string(),
                        json!(string_value(log.get("providerName"))),
                    );
                    base
                }
            )),
        );
        rows.push(Value::Object(row));
    }

    rows.sort_by(|left, right| {
        let usage_order =
            number_value(right.get("usageCount"), 0).cmp(&number_value(left.get("usageCount"), 0));

        if usage_order == std::cmp::Ordering::Equal {
            string_value(left.get("name")).cmp(&string_value(right.get("name")))
        } else {
            usage_order
        }
    });
    rows
}

fn create_skill_trend_stats(invocations: &[SkillInvocation], filters: &Value) -> Vec<Value> {
    let single_day = is_single_day(filters);
    let input_mode = string_value(filters.get("trendMode"));
    let trend_mode = if ["minute", "hour", "day"].contains(&input_mode.as_str()) {
        input_mode
    } else if single_day {
        "hour".to_string()
    } else {
        "day".to_string()
    };
    let mut groups: HashMap<String, (u64, u64, HashMap<String, u64>)> = HashMap::new();

    for invocation in invocations {
        if invocation.created_at == 0 {
            continue;
        }

        let date = local_datetime(invocation.created_at);
        let day = format!("{}-{:02}-{:02}", date.year(), date.month(), date.day());
        let key = if trend_mode == "minute" {
            if single_day {
                format!("{:02}:{:02}", date.hour(), date.minute())
            } else {
                format!("{} {:02}:{:02}", day, date.hour(), date.minute())
            }
        } else if trend_mode == "hour" {
            if single_day {
                format!("{:02}:00", date.hour())
            } else {
                format!("{} {:02}:00", day, date.hour())
            }
        } else {
            day
        };
        let sort_key = if trend_mode == "minute" {
            local_sort_at(invocation.created_at, true, true)
        } else if trend_mode == "hour" {
            local_sort_at(invocation.created_at, true, false)
        } else {
            local_sort_at(invocation.created_at, false, false)
        };

        groups
            .entry(key.clone())
            .or_insert_with(|| (sort_key, 0, HashMap::new()));
        if let Some((_, usage_count, skill_counts)) = groups.get_mut(&key) {
            *usage_count += 1;
            *skill_counts
                .entry(invocation.skill_name.clone())
                .or_insert(0) += 1;
        }
    }

    let mut items = groups
        .into_iter()
        .map(|(date, (sort_key, usage_count, skill_counts))| {
            let mut skills = skill_counts
                .into_iter()
                .map(|(skill_name, usage_count)| {
                    json!({
                      "skillName": skill_name,
                      "usageCount": usage_count
                    })
                })
                .collect::<Vec<_>>();

            skills.sort_by(|left, right| {
                let usage_order = number_value(right.get("usageCount"), 0)
                    .cmp(&number_value(left.get("usageCount"), 0));

                if usage_order == std::cmp::Ordering::Equal {
                    string_value(left.get("skillName")).cmp(&string_value(right.get("skillName")))
                } else {
                    usage_order
                }
            });

            json!({
              "date": date,
              "usageCount": usage_count,
              "skills": skills,
              "sortKey": sort_key
            })
        })
        .collect::<Vec<_>>();

    items.sort_by(|left, right| {
        number_value(left.get("sortKey"), 0).cmp(&number_value(right.get("sortKey"), 0))
    });
    items
        .into_iter()
        .map(|mut item| {
            item.as_object_mut().map(|map| {
                map.remove("sortKey");
            });
            item
        })
        .collect()
}

fn match_invocation_filters(invocation: &SkillInvocation, filters: &Value) -> bool {
    let cli = string_value(filters.get("cli"));
    let start_at = number_value(filters.get("startAt"), 0);
    let end_at = number_value(filters.get("endAt"), 0);

    if cli != "all" && invocation.cli != cli {
        return false;
    }

    if start_at > 0 && invocation.created_at == 0 {
        return false;
    }

    if start_at > 0 && invocation.created_at < start_at {
        return false;
    }

    if end_at > 0 && invocation.created_at == 0 {
        return false;
    }

    if end_at > 0 && invocation.created_at > end_at {
        return false;
    }

    true
}

fn match_log_filters(log: &Value, filters: &Value) -> bool {
    let cli = string_value(filters.get("cli"));

    cli == "all" || string_value(log.get("appType")) == cli
}

fn scan_skill_roots(skills_path: &str) -> Result<Vec<String>, ManagerError> {
    let mut roots = Vec::new();

    for entry in std::fs::read_dir(skills_path)? {
        let entry = entry?;
        let entry_path = entry.path();

        if !entry_path.is_dir() {
            continue;
        }

        if entry_path.join("SKILL.md").exists() {
            roots.push(entry_path.to_string_lossy().to_string());
        }
    }

    Ok(roots)
}

async fn read_skill_name(skill_root: &str) -> Result<String, ManagerError> {
    let skill_file = Path::new(skill_root).join("SKILL.md");
    let content = tokio::fs::read_to_string(skill_file).await?;

    if content.starts_with("---") {
        let mut lines = content.lines();

        lines.next();
        for line in lines {
            let text = line.trim();

            if text == "---" {
                break;
            }

            if let Some(name) = text.strip_prefix("name:") {
                let value = name.trim().trim_matches('"').trim_matches('\'');

                if !value.is_empty() {
                    return Ok(value.to_string());
                }
            }
        }
    }

    Ok(Path::new(skill_root)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_string())
}

fn scan_session_files(
    root_path: &str,
    cli_target: &Value,
    depth: u64,
) -> Result<Vec<String>, ManagerError> {
    if depth > 5 {
        return Ok(Vec::new());
    }

    let mut files = Vec::new();

    for entry in std::fs::read_dir(root_path)? {
        let entry = entry?;
        let entry_path = entry.path();
        let file_name = entry.file_name().to_string_lossy().to_string();

        if entry_path.is_dir() {
            if !["node_modules", ".git", "dist", "build"].contains(&file_name.as_str()) {
                files.extend(scan_session_files(
                    &entry_path.to_string_lossy(),
                    cli_target,
                    depth + 1,
                )?);
            }
            continue;
        }

        if entry_path.is_file() && match_session_file(&file_name, cli_target) {
            files.push(entry_path.to_string_lossy().to_string());
        }
    }

    Ok(files)
}

fn match_session_file(file_name: &str, cli_target: &Value) -> bool {
    let rules = session_scan_rules(cli_target);
    let extension = Path::new(file_name)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| format!(".{}", value))
        .unwrap_or_default();
    let extensions = rules
        .get("extensions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| string_value(Some(&item)))
        .collect::<Vec<_>>();
    let names = rules
        .get("names")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| string_value(Some(&item)))
        .collect::<Vec<_>>();

    extensions.contains(&extension)
        && (names.is_empty() || names.iter().any(|item| file_name.starts_with(item)))
}

fn session_scan_rules(cli_target: &Value) -> Value {
    if let Some(rules) = cli_target.get("sessionScanRules") {
        return rules.clone();
    }

    match get_cli_type(cli_target).as_str() {
        "claude" => json!({ "extensions": [".jsonl"], "names": [] }),
        "codex" => json!({ "extensions": [".json", ".jsonl", ".transcript"], "names": [] }),
        "gemini" => {
            json!({ "extensions": [".json", ".jsonl"], "names": ["session", "checkpoint"] })
        }
        "opencode" => json!({ "extensions": [".json", ".jsonl", ".transcript"], "names": [] }),
        _ => json!({ "extensions": [".json", ".jsonl", ".transcript"], "names": [] }),
    }
}

fn get_session_paths(cli_target: &Value) -> Vec<String> {
    let session_paths = cli_target
        .get("sessionPaths")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| string_value(Some(&item)))
        .filter(|item| !item.is_empty())
        .collect::<Vec<_>>();

    if !session_paths.is_empty() {
        return session_paths;
    }

    let sessions_path = string_value(cli_target.get("sessionsPath"));

    if sessions_path.is_empty() {
        Vec::new()
    } else {
        vec![sessions_path]
    }
}

fn collect_gemini_usage_items<'a>(source: &'a Value, output: &mut Vec<&'a Value>) {
    if source.get("usageMetadata").is_some() {
        output.push(source);
    }

    match source {
        Value::Array(items) => {
            for item in items {
                collect_gemini_usage_items(item, output);
            }
        }
        Value::Object(map) => {
            for value in map.values() {
                collect_gemini_usage_items(value, output);
            }
        }
        _ => {}
    }
}

fn session_content_created_at(
    app_type: &str,
    extension: &str,
    content: &str,
) -> Result<u64, ManagerError> {
    let mut session_created_at = 0;

    if app_type == "gemini" {
        let payload: Value = serde_json::from_str(content)?;
        let mut items = Vec::new();

        collect_gemini_usage_items(&payload, &mut items);

        for item in items {
            let created_at = to_timestamp(
                item.get("createTime")
                    .or_else(|| item.get("timestamp"))
                    .or_else(|| payload.get("updatedAt")),
                0,
            );

            if created_at > 0 && (session_created_at == 0 || created_at < session_created_at) {
                session_created_at = created_at;
            }
        }
    } else if !(app_type == "codex" && extension == "json") {
        for line in content.lines() {
            let text = line.trim();

            if text.is_empty() {
                continue;
            }

            let record: Value = serde_json::from_str(text)?;
            let payload = record.get("payload").unwrap_or(&record);
            let created_at = to_timestamp(
                record
                    .get("timestamp")
                    .or_else(|| payload.get("timestamp"))
                    .or_else(|| payload.get("createdAt")),
                0,
            );

            if created_at > 0 && (session_created_at == 0 || created_at < session_created_at) {
                session_created_at = created_at;
            }
        }
    }

    Ok(session_created_at)
}

fn normalize_codex_token_usage(usage: Option<&Value>) -> Map<String, Value> {
    let mut map = Map::new();

    map.insert(
        "input_tokens".to_string(),
        json!(to_number(usage.and_then(|item| item.get("input_tokens")))),
    );
    map.insert(
        "cached_input_tokens".to_string(),
        json!(to_number(
            usage.and_then(|item| item.get("cached_input_tokens"))
        )),
    );
    map.insert(
        "output_tokens".to_string(),
        json!(to_number(usage.and_then(|item| item.get("output_tokens")))),
    );
    map
}

fn subtract_token_usage(
    current: &Map<String, Value>,
    previous: Option<&Map<String, Value>>,
) -> Map<String, Value> {
    let Some(previous) = previous else {
        return current.clone();
    };
    let mut map = Map::new();

    for key in ["input_tokens", "cached_input_tokens", "output_tokens"] {
        let value =
            number_value(current.get(key), 0) as i64 - number_value(previous.get(key), 0) as i64;

        map.insert(key.to_string(), json!(value));
    }

    map
}

fn is_valid_codex_delta(delta: &Map<String, Value>) -> bool {
    let input_tokens = signed_number(delta.get("input_tokens"));
    let cached_input_tokens = signed_number(delta.get("cached_input_tokens"));
    let output_tokens = signed_number(delta.get("output_tokens"));

    input_tokens >= 0
        && cached_input_tokens >= 0
        && output_tokens >= 0
        && input_tokens + output_tokens > 0
}

fn get_usage_value(usage: &Value, keys: &[&str]) -> u64 {
    for key in keys {
        if let Some(value) = usage.get(*key) {
            return to_number(Some(value));
        }
    }

    0
}

fn normalize_billable_input(log: &Value) -> u64 {
    let app_type = string_value(log.get("appType"));
    let input_tokens = to_number(log.get("inputTokens"));
    let cache_read_tokens = to_number(log.get("cacheReadTokens"));

    if app_type == "codex" || app_type == "gemini" {
        input_tokens.saturating_sub(cache_read_tokens)
    } else {
        input_tokens
    }
}

fn to_actual_tokens(log: &Value) -> u64 {
    normalize_billable_input(log)
        + to_number(log.get("outputTokens"))
        + to_number(log.get("cacheReadTokens"))
        + to_number(log.get("cacheCreationTokens"))
}

fn create_pricing_index(pricing_config: &Value) -> HashMap<String, Value> {
    let mut index = HashMap::new();

    for item in pricing_config
        .get("items")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let model_id = string_value(item.get("modelId")).trim().to_lowercase();

        if !model_id.is_empty() && !index.contains_key(&model_id) {
            index.insert(model_id, item.clone());
        }
    }

    index
}

fn find_model_pricing(log: &Value, pricing_index: &HashMap<String, Value>) -> Option<Value> {
    let model_keys = [
        string_value(log.get("model")),
        string_value(log.get("requestModel")),
    ]
    .into_iter()
    .map(|item| item.trim().to_lowercase())
    .filter(|item| !item.is_empty())
    .collect::<Vec<_>>();

    model_keys
        .into_iter()
        .find_map(|model_key| pricing_index.get(&model_key).cloned())
}

fn price_to_usd(value: f64, currency: &str, exchange_rate: f64) -> f64 {
    if currency == "CNY" {
        value / exchange_rate
    } else {
        value
    }
}

fn create_empty_summary() -> Summary {
    Summary {
        request_count: 0,
        usage_count: 0,
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_creation_tokens: 0,
        actual_tokens: 0,
        total_cost_usd: 0.0,
        last_used_at: 0,
    }
}

fn merge_summary(mut base: Map<String, Value>, summary: Value) -> Value {
    if let Some(summary) = summary.as_object() {
        for (key, value) in summary {
            base.insert(key.clone(), value.clone());
        }
    }

    Value::Object(base)
}

fn cli_filter_options(cli_targets: &[Value]) -> Vec<Value> {
    let mut options = Vec::new();
    let mut seen = HashMap::new();

    for item in cli_targets {
        let id = get_cli_type(item);

        if seen.contains_key(&id) {
            continue;
        }

        seen.insert(id.clone(), true);
        options.push(json!({
          "id": id,
          "name": non_empty_text(item.get("name"), &id)
        }));
    }

    options
}

fn get_cli_type(cli_target: &Value) -> String {
    let cli_type = string_value(cli_target.get("type"));

    if !cli_type.is_empty() {
        return cli_type;
    }

    let cli = string_value(cli_target.get("cli"));

    if !cli.is_empty() {
        return cli;
    }

    string_value(cli_target.get("id"))
}

fn get_session_record_role(record: &Value) -> String {
    let payload = record.get("payload").unwrap_or(record);
    let message = record
        .get("message")
        .or_else(|| payload.get("message"))
        .unwrap_or(payload);
    let role = string_value(message.get("role"));

    if !role.is_empty() {
        return role;
    }

    let role = string_value(payload.get("role"));

    if !role.is_empty() {
        return role;
    }

    let role = string_value(record.get("role"));

    if !role.is_empty() {
        return role;
    }

    first_text(payload.get("type"), record.get("type"), "")
}

fn first_defined_value(values: &[Option<&Value>]) -> Value {
    for value in values.iter().flatten() {
        if !value.is_null() && !string_value(Some(value)).is_empty() {
            return (*value).clone();
        }
    }

    Value::Null
}

fn first_text(value: Option<&Value>, fallback: Option<&Value>, default_value: &str) -> String {
    let text = string_value(value);

    if !text.is_empty() {
        return text;
    }

    let text = string_value(fallback);

    if !text.is_empty() {
        return text;
    }

    default_value.to_string()
}

fn first_positive_number(values: &[Option<&Value>]) -> u64 {
    for value in values.iter().flatten() {
        let number = to_number(Some(value));

        if number > 0 {
            return number;
        }
    }

    0
}

fn proxy_state_enabled(state: &Value, cli: &str) -> bool {
    state
        .get(&format!("{}ProxyState", cli))
        .and_then(|item| item.get("enabled"))
        .and_then(Value::as_bool)
        == Some(true)
}

fn normalize_usage_trend_mode(value: Option<&Value>) -> String {
    let mode = string_value(value);

    if mode == "hour" || mode == "minute" {
        mode
    } else {
        "day".to_string()
    }
}

fn normalize_skill_trend_mode(value: Option<&Value>) -> String {
    let mode = string_value(value);

    if mode == "hour" || mode == "minute" || mode == "day" {
        mode
    } else {
        String::new()
    }
}

fn normalize_app_type(value: &str) -> String {
    value.trim().to_lowercase()
}

fn normalize_currency(value: Option<&Value>) -> String {
    if string_value(value).to_uppercase() == "CNY" {
        "CNY".to_string()
    } else {
        "USD".to_string()
    }
}

fn normalize_model_category(value: Option<&Value>) -> String {
    string_value(value)
}

fn format_app_provider_name(cli: &str) -> String {
    match cli {
        "claude" => "Claude".to_string(),
        "codex" => "Codex".to_string(),
        "gemini" => "Gemini".to_string(),
        "" => "未知 CLI".to_string(),
        _ => cli.to_string(),
    }
}

fn non_empty_text(value: Option<&Value>, fallback: &str) -> String {
    non_empty_owned(string_value(value), fallback)
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| string_value(Some(item)))
                .filter(|item| !item.trim().is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn provider_stats_cache_key(filters: &Value) -> String {
    let mut provider_ids = filters
        .get("providerIds")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|item| string_value(Some(&item)))
        .filter(|item| !item.trim().is_empty())
        .collect::<Vec<_>>();

    provider_ids.sort();

    format!(
        "{}|{}|{}",
        string_value(filters.get("appType")),
        non_empty_owned(provider_ids.join(","), &string_value(filters.get("providerId"))),
        string_value(filters.get("requestSource"))
    )
}

fn usage_log_cache_id(log: &Value) -> String {
    let request_id = string_value(log.get("requestId"));

    if !request_id.is_empty() {
        return request_id;
    }

    create_hash_id(&[
        string_value(log.get("rawPath")),
        string_value(log.get("createdAt")),
        string_value(log.get("providerId")),
        string_value(log.get("model")),
    ])
}

fn usage_log_cache_signature(log: &Value) -> String {
    create_hash_id(&[
        string_value(log.get("createdAt")),
        string_value(log.get("appType")),
        string_value(log.get("providerId")),
        string_value(log.get("providerName")),
        string_value(log.get("providerType")),
        string_value(log.get("model")),
        string_value(log.get("requestSource")),
        string_value(log.get("inputTokens")),
        string_value(log.get("outputTokens")),
        string_value(log.get("cacheReadTokens")),
        string_value(log.get("cacheCreationTokens")),
        string_value(log.get("actualTokens")),
        string_value(log.get("totalCostUsd")),
    ])
}

fn non_empty_owned(value: String, fallback: &str) -> String {
    if value.trim().is_empty() {
        fallback.to_string()
    } else {
        value
    }
}

fn to_number(value: Option<&Value>) -> u64 {
    match value {
        Some(Value::Number(number)) => number.as_f64().unwrap_or(0.0).floor().max(0.0) as u64,
        Some(Value::String(text)) => text.parse::<f64>().unwrap_or(0.0).floor().max(0.0) as u64,
        Some(Value::Bool(value)) => {
            if *value {
                1
            } else {
                0
            }
        }
        _ => 0,
    }
}

fn signed_number(value: Option<&Value>) -> i64 {
    match value {
        Some(Value::Number(number)) => number
            .as_i64()
            .unwrap_or_else(|| number.as_f64().unwrap_or(0.0) as i64),
        Some(Value::String(text)) => text.parse::<i64>().unwrap_or(0),
        _ => 0,
    }
}

fn number_value(value: Option<&Value>, fallback: u64) -> u64 {
    match value {
        Some(Value::Number(number)) => number.as_u64().unwrap_or(fallback),
        Some(Value::String(text)) => text.parse::<u64>().unwrap_or(fallback),
        _ => fallback,
    }
}

fn price_number(value: Option<&Value>, fallback: f64) -> f64 {
    let number = match value {
        Some(Value::Number(number)) => number.as_f64().unwrap_or(fallback),
        Some(Value::String(text)) => text.parse::<f64>().unwrap_or(fallback),
        _ => fallback,
    };

    if number.is_finite() {
        number.max(0.0)
    } else {
        0.0
    }
}

fn value_to_text(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(text)) => text.clone(),
        Some(Value::Number(number)) => number.to_string(),
        Some(Value::Bool(value)) => value.to_string(),
        Some(Value::Null) | None => String::new(),
        Some(value) => value.to_string(),
    }
}

fn to_timestamp(value: Option<&Value>, fallback: u64) -> u64 {
    match value {
        Some(Value::Number(number)) => {
            let number = number.as_u64().unwrap_or(fallback);

            if number > 1_000_000_000_000 {
                number
            } else {
                number * 1000
            }
        }
        Some(Value::String(text)) => {
            if let Ok(number) = text.parse::<u64>() {
                return if number > 1_000_000_000_000 {
                    number
                } else {
                    number * 1000
                };
            }

            chrono::DateTime::parse_from_rfc3339(text)
                .map(|date| date.timestamp_millis().max(0) as u64)
                .unwrap_or(fallback)
        }
        _ => fallback,
    }
}

fn to_timestamp_ms(value: Option<&Value>, fallback: u64) -> u64 {
    match value {
        Some(Value::Number(number)) => {
            let number = number.as_u64().unwrap_or(fallback);

            if number > 1_000_000_000_000 {
                number
            } else {
                number * 1000
            }
        }
        Some(Value::String(text)) => {
            if let Ok(number) = text.parse::<u64>() {
                return if number > 1_000_000_000_000 {
                    number
                } else {
                    number * 1000
                };
            }

            chrono::DateTime::parse_from_rfc3339(text)
                .map(|date| date.timestamp_millis().max(0) as u64)
                .unwrap_or(fallback)
        }
        _ => fallback,
    }
}

fn is_single_day(filters: &Value) -> bool {
    let start_at = number_value(filters.get("startAt"), 0);
    let end_at = number_value(filters.get("endAt"), 0);

    if start_at == 0 || end_at == 0 {
        return false;
    }

    let start = local_datetime(start_at);
    let end = local_datetime(end_at);

    start.year() == end.year() && start.month() == end.month() && start.day() == end.day()
}

fn today_start_at() -> u64 {
    let now = Local::now();

    Local
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .map(|date| date.timestamp_millis().max(0) as u64)
        .unwrap_or_else(|| local_sort_at(now.timestamp_millis().max(0) as u64, false, false))
}

fn local_datetime(timestamp: u64) -> chrono::DateTime<Local> {
    Local
        .timestamp_millis_opt(timestamp as i64)
        .single()
        .unwrap_or_else(Local::now)
}

fn local_sort_at(timestamp: u64, include_hour: bool, include_minute: bool) -> u64 {
    let date = local_datetime(timestamp);
    let hour = if include_hour { date.hour() } else { 0 };
    let minute = if include_minute { date.minute() } else { 0 };

    Local
        .with_ymd_and_hms(date.year(), date.month(), date.day(), hour, minute, 0)
        .single()
        .map(|date| date.timestamp_millis().max(0) as u64)
        .unwrap_or(timestamp)
}

fn format_zh_cn_date(timestamp: u64) -> String {
    let date = local_datetime(timestamp);

    format!("{}/{}/{}", date.year(), date.month(), date.day())
}

fn unique_strings(items: Vec<String>) -> Vec<String> {
    let mut result = Vec::new();

    for item in items {
        if !item.is_empty() && !result.contains(&item) {
            result.push(item);
        }
    }

    result
}

fn create_hash_id(parts: &[String]) -> String {
    let mut hasher = Sha1::new();

    hasher.update(parts.join("|").as_bytes());
    format!("{:x}", hasher.finalize())
}

fn create_pricing_id() -> String {
    format!(
        "pricing-{}-{}",
        now_millis(),
        PRICING_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn slash_skill_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();

    REGEX.get_or_init(|| Regex::new(r#"(?:^|[\s`"'(\[\{<])/([A-Za-z0-9][A-Za-z0-9._-]*)"#).unwrap())
}

fn path_skill_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();

    REGEX.get_or_init(|| {
        Regex::new(r#"(?i)(?:^|[\\/])skills[\\/](?:\.system[\\/])?([^\\/]+)[\\/]SKILL\.md"#)
            .unwrap()
    })
}

fn decode_report_image_data_url(value: &str) -> Result<Vec<u8>, ManagerError> {
    let Some((metadata, data)) = value.trim().split_once(',') else {
        return Err(ManagerError::System("用量报告图片数据格式无效。".to_string()));
    };
    let metadata = metadata.to_ascii_lowercase();

    if !metadata.starts_with("data:image/png") || !metadata.contains(";base64") {
        return Err(ManagerError::System(
            "用量报告只支持导出 PNG 图片。".to_string(),
        ));
    }

    if data.trim().is_empty() {
        return Err(ManagerError::System("用量报告图片数据为空。".to_string()));
    }

    BASE64_STANDARD
        .decode(data.trim())
        .map_err(|error| ManagerError::System(format!("用量报告图片数据解析失败：{error}")))
}

#[cfg(test)]
mod tests {
    use super::{
        collect_session_record_texts, decode_report_image_data_url, file_modified_at,
        is_codex_subagent_session, path_skill_regex, refresh_usage, slash_skill_regex,
    };
    use crate::core::{paths::resolve_app_paths, usage_store};
    use serde_json::json;
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn skill_usage_regexes_are_valid() {
        assert!(slash_skill_regex().is_match("/frontend-design"));
        assert!(path_skill_regex().is_match(r"C:\Users\readboy\.codex\skills\frontend-design\SKILL.md"));
    }

    #[test]
    fn collects_custom_tool_call_input_for_skill_usage() {
        let record = json!({
          "payload": {
            "type": "custom_tool_call",
            "input": r"Get-Content C:\Users\readboy\.codex\skills\frontend-design\SKILL.md"
          }
        });
        let mut texts = Vec::new();

        collect_session_record_texts(&record, &mut texts);

        assert_eq!(
            texts,
            vec![r"Get-Content C:\Users\readboy\.codex\skills\frontend-design\SKILL.md"]
        );
    }

    #[test]
    fn decodes_png_report_image_data_url() {
        let bytes = decode_report_image_data_url("data:image/png;base64,aGVsbG8=").unwrap();

        assert_eq!(bytes, b"hello");
    }

    #[test]
    fn rejects_non_png_report_image_data_url() {
        let error = decode_report_image_data_url("data:text/plain;base64,aGVsbG8=")
            .expect_err("non-png data URL should be rejected");

        assert!(error.to_string().contains("PNG"));
    }

    #[test]
    fn detects_codex_subagent_session_from_metadata() {
        let root = std::env::temp_dir().join(format!(
            "monkey-thief-codex-session-{}-{}",
            std::process::id(),
            super::now_millis()
        ));
        std::fs::create_dir_all(&root).unwrap();

        let subagent_path = root.join("subagent.jsonl");
        std::fs::write(
            &subagent_path,
            r#"{"timestamp":"2026-07-17T10:00:00Z","type":"session_meta","payload":{"session_id":"parent","id":"child","thread_source":"subagent","parent_thread_id":"parent"}}
"#,
        )
        .unwrap();
        assert!(is_codex_subagent_session(subagent_path.to_string_lossy().as_ref()).unwrap());

        let root_path = root.join("root.jsonl");
        std::fs::write(
            &root_path,
            r#"{"timestamp":"2026-07-17T10:00:00Z","type":"session_meta","payload":{"session_id":"root","id":"root"}}
"#,
        )
        .unwrap();
        assert!(!is_codex_subagent_session(root_path.to_string_lossy().as_ref()).unwrap());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn refresh_usage_removes_indexed_codex_subagent_usage() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "monkey-thief-codex-subagent-usage-{}-{}",
                std::process::id(),
                super::now_millis()
            ));
            let paths = resolve_app_paths(Path::new(&root));
            let subagent_path = root.join("subagent.jsonl");

            std::fs::create_dir_all(&paths.workspace_root).unwrap();
            std::fs::write(
                &subagent_path,
                r#"{"timestamp":"2026-07-17T10:00:00Z","type":"session_meta","payload":{"session_id":"parent","id":"child","thread_source":"subagent","parent_thread_id":"parent"}}
"#,
            )
            .unwrap();
            let subagent_path = subagent_path.to_string_lossy().to_string();
            let updated_at = file_modified_at(&subagent_path);

            usage_store::replace_sessions(
                &paths,
                &[usage_store::UsageSessionUpdate {
                    raw_path: subagent_path.clone(),
                    app_type: "codex".to_string(),
                    updated_at,
                    logs: vec![json!({
                      "requestId": "subagent-log",
                      "rawPath": subagent_path.clone(),
                      "createdAt": updated_at,
                      "appType": "codex"
                    })],
                    records: Vec::new(),
                }],
            )
            .unwrap();

            let state = json!({
              "providers": [],
              "sessions": [{
                "id": "subagent-session",
                "cli": "codex",
                "rawPath": subagent_path,
                "updatedAt": updated_at
              }]
            });

            refresh_usage(&paths, &state).await.unwrap();

            assert!(usage_store::read_all_logs(&paths).unwrap().is_empty());
            assert!(usage_store::read_session_versions(&paths)
                .unwrap()
                .is_empty());

            let _ = std::fs::remove_dir_all(root);
        });
    }

    #[test]
    fn refresh_usage_reprocesses_indexed_session_after_file_changes() {
        tauri::async_runtime::block_on(async {
            let root = std::env::temp_dir().join(format!(
                "monkey-thief-usage-refresh-{}-{}",
                std::process::id(),
                super::now_millis()
            ));
            let paths = resolve_app_paths(Path::new(&root));
            let session_path = root.join("session.jsonl");

            std::fs::create_dir_all(&paths.workspace_root).unwrap();
            std::fs::write(
                &session_path,
                concat!(
                    "{\"timestamp\":\"2026-07-17T10:00:00Z\",\"payload\":{\"type\":\"session_meta\",\"model\":\"gpt-test\"}}\n",
                    "{\"timestamp\":\"2026-07-17T10:00:01Z\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":10,\"output_tokens\":5}}}}\n"
                ),
            )
            .unwrap();
            let indexed_at = file_modified_at(session_path.to_string_lossy().as_ref());
            let state = json!({
              "providers": [],
              "sessions": [{
                "id": "session-a",
                "cli": "codex",
                "rawPath": session_path.to_string_lossy(),
                "updatedAt": indexed_at
              }]
            });

            refresh_usage(&paths, &state).await.unwrap();
            std::thread::sleep(Duration::from_millis(20));
            std::fs::write(
                &session_path,
                concat!(
                    "{\"timestamp\":\"2026-07-17T10:00:00Z\",\"payload\":{\"type\":\"session_meta\",\"model\":\"gpt-test\"}}\n",
                    "{\"timestamp\":\"2026-07-17T10:00:01Z\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":10,\"output_tokens\":5}}}}\n",
                    "{\"timestamp\":\"2026-07-17T10:00:02Z\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":30,\"output_tokens\":10}}}}\n"
                ),
            )
            .unwrap();

            refresh_usage(&paths, &state).await.unwrap();

            assert_eq!(usage_store::read_all_logs(&paths).unwrap().len(), 2);
            let _ = std::fs::remove_dir_all(root);
        });
    }

}

fn read_usage_logs(paths: &AppPaths) -> Result<Arc<Vec<Value>>, ManagerError> {
    let revision = usage_store::revision(paths)?;
    let path = &paths.storage_files.database;
    let cache = USAGE_LOG_CACHE.get_or_init(|| Mutex::new(None));
    let mut cache = cache
        .lock()
        .map_err(|error| ManagerError::System(error.to_string()))?;

    if let Some(cache) = cache.as_ref() {
        if cache.path == *path && cache.revision == revision {
            return Ok(cache.logs.clone());
        }
    }

    let logs = Arc::new(usage_store::read_all_logs(paths)?);

    *cache = Some(UsageLogCache {
        path: path.clone(),
        revision,
        logs: logs.clone(),
    });
    Ok(logs)
}

fn round_to(value: f64, digits: i32) -> f64 {
    let factor = 10_f64.powi(digits);

    (value * factor).round() / factor
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

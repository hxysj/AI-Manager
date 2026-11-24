use super::*;
use crate::core::{database, service_provider_store as store};

// 只投影新账本；旧 Usage 表、归属和锁价均不因模式切换而迁移或覆盖。
pub(super) struct Snapshot {
    pub revision: String,
    logs: Vec<Value>,
    managed_periods: Vec<Value>,
}

pub(super) fn read(paths: &AppPaths) -> Result<Snapshot, ManagerError> {
    read_snapshot(paths, None, &mut Vec::new())
}

pub(super) fn merge_calendar_logs(
    paths: &AppPaths,
    mut legacy: Vec<Value>,
    start: u64,
    end: u64,
) -> Result<Vec<Value>, ManagerError> {
    let start = i64::try_from(start).map_err(|error| store::fail(error.to_string()))?;
    let end = i64::try_from(end).map_err(|error| store::fail(error.to_string()))?;
    let snapshot = read_snapshot(paths, Some((start, end)), &mut legacy)?;
    Ok(snapshot.merge(legacy))
}

fn read_snapshot(
    paths: &AppPaths,
    range: Option<(i64, i64)>,
    legacy: &mut Vec<Value>,
) -> Result<Snapshot, ManagerError> {
    let c = database::open(paths)?;
    let exists: bool = c.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_schema WHERE type='table' AND name='service_gateway_requests')",
        [], |row| row.get(0),
    )?;
    if !exists {
        return Ok(Snapshot { revision: String::new(), logs: Vec::new(), managed_periods: Vec::new() });
    }
    // 在同一快照内读取请求与接管时段，避免流结束或回滚时读到一半状态。
    c.execute_batch("BEGIN DEFERRED")?;
    let date_condition = if range.is_some() {
        "AND r.created_at BETWEEN ?1 AND ?2"
    } else {
        ""
    };
    let (start, end) = range.unwrap_or_default();
    let parameters: &[&dyn rusqlite::ToSql] = if range.is_some() { &[&start, &end] } else { &[] };
    let logs = store::query(&c, &format!("SELECT r.id,r.agent_type AS appType,r.created_at AS createdAt,
        r.finished_at AS finishedAt,r.state,r.status_code AS statusCode,r.error_message AS errorMessage,
        r.requested_model AS requestModel,r.client_request_id AS clientRequestId,
        coalesce(a.provider_id,i.provider_id,'') AS providerId,
        coalesce(a.provider_name,p.name,'未路由请求') AS providerName,p.kind AS providerKind,
        a.actual_model AS actualModel,a.upstream_model AS model,a.upstream_response_id AS responseId,
        a.input_tokens AS inputTokens,a.output_tokens AS outputTokens,a.cached_input_tokens AS cacheReadTokens,
        a.credential_id AS apiKeyId,a.credential_fingerprint AS apiKeyHash,
        a.estimated_cost_micros AS estimatedCostMicros,a.currency,
        a.price_snapshot_json AS servicePricingJson,i.config_path AS configPath,i.name AS instanceName
        FROM service_gateway_requests r
        LEFT JOIN service_gateway_attempts a ON a.request_id=r.id AND a.attempt_no=(
            SELECT max(x.attempt_no) FROM service_gateway_attempts x WHERE x.request_id=r.id)
        LEFT JOIN service_agent_instances i ON i.id=r.instance_id
        LEFT JOIN service_provider_configs p ON p.id=coalesce(a.provider_id,i.provider_id)
        WHERE r.finished_at IS NOT NULL {date_condition} ORDER BY r.created_at DESC,r.id"), parameters)?;
    if range.is_some() {
        // 会话和网关的记账时间可能跨过日期边界，按身份索引查找区间外的重复项。
        let mut duplicate = c.prepare("SELECT EXISTS(
            SELECT 1 FROM service_gateway_requests r
            WHERE replace(r.id,'-','')=?1 AND r.agent_type=?3
            AND r.finished_at IS NOT NULL AND r.created_at NOT BETWEEN ?4 AND ?5
            UNION ALL
            SELECT 1 FROM service_gateway_attempts a
            JOIN service_gateway_requests r ON r.id=a.request_id
            WHERE a.upstream_response_id=?2 AND a.upstream_response_id<>''
            AND r.agent_type=?3 AND r.finished_at IS NOT NULL AND r.created_at NOT BETWEEN ?4 AND ?5
            AND a.attempt_no=(SELECT max(x.attempt_no) FROM service_gateway_attempts x WHERE x.request_id=r.id)
        )")?;
        let mut retained = Vec::with_capacity(legacy.len());
        for log in std::mem::take(legacy) {
            if let Some(response) = store::text(&log, "requestId").strip_prefix("session:") {
                let synthetic_id = response.strip_prefix("msg_aim_");
                let found = duplicate.query_row(
                    rusqlite::params![synthetic_id, response, store::text(&log, "appType"), start, end],
                    |row| row.get::<_, bool>(0),
                )?;
                if found {
                    continue;
                }
            }
            retained.push(log);
        }
        *legacy = retained;
    }
    let managed_periods = store::query(&c, "SELECT i.agent_type AS appType,i.config_path AS configPath,
        f.created_at AS startAt,CASE WHEN f.restore_status='restored' THEN f.updated_at END AS endAt
        FROM service_managed_files f JOIN service_agent_instances i ON i.id=f.instance_id
        WHERE f.write_status='written' AND i.agent_type IN ('codex','claude')
        AND (f.file_path LIKE '%config.toml' OR f.file_path LIKE '%settings.json')", &[])?;
    c.execute_batch("COMMIT")?;
    let revision = if range.is_some() {
        String::new()
    } else {
        store::hash(&json!([logs, managed_periods]).to_string())
    };
    Ok(Snapshot { revision, logs, managed_periods })
}

impl Snapshot {
    pub(super) fn merge(self, mut legacy: Vec<Value>) -> Vec<Value> {
        let response_ids = self.logs.iter().flat_map(|log| {
            let response = store::text(log, "responseId");
            let app = store::text(log, "appType").to_string();
            let mut ids = vec![(app.clone(), format!("session:msg_aim_{}", store::text(log, "id").replace('-', "")))];
            if !response.is_empty() {
                ids.push((app, format!("session:{response}")));
            }
            ids
        }).collect::<HashSet<_>>();
        legacy.retain(|log| {
            let identity = (string_value(log.get("appType")), string_value(log.get("requestId")));
            // 有响应 ID 时精确去重；Codex token_count 没有响应 ID，按接管目录和时段采用网关账本。
            log["dataSource"] != "service-gateway-session"
                && !response_ids.contains(&identity)
                && !self.managed_periods.iter().any(|period| {
                log["appType"] == period["appType"]
                    && number_value(log.get("createdAt"), 0) >= number_value(period.get("startAt"), 0)
                    && (period["endAt"].is_null()
                        || number_value(log.get("createdAt"), 0) < number_value(period.get("endAt"), 0))
                    && within_directory(store::text(log, "rawPath"), store::text(period, "configPath"))
            })
        });
        legacy.extend(self.logs.into_iter().map(project));
        legacy.sort_by(|a, b| number_value(b.get("createdAt"), 0).cmp(&number_value(a.get("createdAt"), 0))
            .then_with(|| store::text(a, "requestId").cmp(store::text(b, "requestId"))));
        legacy
    }
}

// Desktop 审计按回合汇总多个响应；全回合均来自网关时保留会话记录，由账本参与统计。
pub(super) fn mark_desktop_logs(paths: &AppPaths, logs: &mut [Value]) -> Result<(), ManagerError> {
    if !logs.iter().any(|log| log["desktopResponseIds"].as_array().is_some_and(|ids|
        ids.iter().any(|id| id.as_str().is_some_and(|id| id.starts_with("msg_aim_"))))) {
        return Ok(());
    }
    let requests = store::query(&database::open(paths)?,
        "SELECT id FROM service_gateway_requests WHERE agent_type='claude-desktop'", &[])?;
    let ids = requests.iter()
        .map(|log| format!("msg_aim_{}", store::text(log, "id").replace('-', ""))).collect::<HashSet<_>>();
    for log in logs {
        if log["desktopResponseIds"].as_array().is_some_and(|responses| !responses.is_empty()
            && responses.iter().all(|id| id.as_str().is_some_and(|id| ids.contains(id)))) {
            log["dataSource"] = json!("service-gateway-session");
        }
    }
    Ok(())
}

fn within_directory(path: &str, directory: &str) -> bool {
    if path.is_empty() || directory.is_empty() || directory.contains(':') && !Path::new(directory).is_absolute() {
        return false;
    }
    let normalize = |value: &str| {
        let value = value.replace('\\', "/").trim_end_matches('/').to_string();
        if cfg!(windows) {
            let value = value.to_lowercase();
            if let Some(path) = value.strip_prefix("//?/unc/") {
                format!("//{path}")
            } else {
                value.strip_prefix("//?/").unwrap_or(&value).to_string()
            }
        } else { value }
    };
    let directory = normalize(directory);
    let path = normalize(path);
    path.starts_with(&format!("{directory}/"))
}

fn project(mut log: Value) -> Value {
    log["requestId"] = json!(format!("service:{}", store::text(&log, "id")));
    if store::text(&log, "appType").is_empty() {
        log["appType"] = json!("gateway");
    }
    log["requestSource"] = json!("service-gateway");
    log["dataSource"] = json!("service-gateway");
    log["providerType"] = json!(if log["providerKind"] == "codex_account" { "codex" } else { "openai" });
    if !store::text(&log, "actualModel").is_empty() {
        log["model"] = log["actualModel"].clone();
    }
    let input = log["inputTokens"].as_u64();
    let output = log["outputTokens"].as_u64();
    let cached = log["cacheReadTokens"].as_u64();
    log["usageKnown"] = json!(input.is_some() && output.is_some());
    log["actualTokens"] = json!(input.zip(output).map(|(i, o)| i + o));
    // 旧 Usage 的 Claude 输入字段不含缓存；网关的 OpenAI 输入包含缓存。
    if !matches!(store::text(&log, "appType"), "codex" | "gemini") {
        log["inputTokens"] = json!(input.zip(cached).map(|(i, k)| i.saturating_sub(k)));
    }
    log["cacheCreationTokens"] = json!(0);
    log["latencyMs"] = json!(number_value(log.get("finishedAt"), 0)
        .saturating_sub(number_value(log.get("createdAt"), 0)));
    log["sessionTitle"] = log["instanceName"].clone();
    log
}

// 保留网关的未知值；有服务商锁价时使用该快照，否则沿用 Usage 中已有的模型费用配置。
pub(super) fn enrich(log: &Value, pricing: &Value, index: &HashMap<String, Value>) -> Value {
    let mut result = log.clone();
    let complete = ["inputTokens", "outputTokens", "cacheReadTokens"]
        .iter().all(|key| log[*key].is_number());
    let costs = if let Some(cost) = log["estimatedCostMicros"].as_f64() {
        let currency = store::text(log, "currency");
        let converted = price_to_usd(cost / 1_000_000.0, currency,
            log["servicePricing"]["usagePricingSnapshot"]["exchangeRate"].as_f64()
                .unwrap_or_else(|| price_number(pricing.get("exchangeRate"), DEFAULT_EXCHANGE_RATE)));
        json!({"totalCostUsd":converted})
    } else if complete && !["inputPriceMicros", "outputPriceMicros", "cachedInputPriceMicros"]
        .iter().any(|key| log["servicePricing"][*key].is_number())
        && find_model_pricing(log, index).is_some() {
        calculate_cost_usd(log, pricing, index)
    } else {
        json!({})
    };
    for key in ["inputCostUsd", "outputCostUsd", "cacheReadCostUsd", "cacheCreationCostUsd", "totalCostUsd"] {
        result[key] = costs[key].clone();
    }
    result
}

// 修改 Usage 模型费用前，给采用旧费用配置的新请求锁价，避免历史随新价格变化。
pub(super) fn lock_costs(paths: &AppPaths, pricing: &Value, index: &HashMap<String, Value>) -> Result<(), ManagerError> {
    let logs = read(paths)?.logs.into_iter().map(project).filter_map(|log| {
        if !log["estimatedCostMicros"].is_null() { return None; }
        let cost = enrich(&log, pricing, index)["totalCostUsd"].as_f64()?;
        let model = find_model_pricing(&log, index)?;
        Some((store::text(&log, "id").to_string(), (cost * 1_000_000.0).ceil() as i64,
            json!({"model":model,"exchangeRate":pricing["exchangeRate"]}).to_string()))
    }).collect::<Vec<_>>();
    if logs.is_empty() { return Ok(()); }
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    for (id, cost, snapshot) in logs {
        tx.execute("UPDATE service_gateway_attempts SET estimated_cost_micros=?2,currency='USD',
            price_snapshot_json=json_set(price_snapshot_json,'$.usagePricingSnapshot',json(?3))
            WHERE request_id=?1 AND estimated_cost_micros IS NULL AND finished_at IS NOT NULL",
            rusqlite::params![id,cost,snapshot])?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    use crate::api::service_provider;
    use crate::core::paths::resolve_app_paths;

    struct Fixture { paths: AppPaths, root: std::path::PathBuf, provider: String, model: String }
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!("ai-manager-service-usage-{}", store::id()));
            let paths = resolve_app_paths(&root);
            let provider = store::save(&paths, &json!({
                "name":"共享服务商","baseUrl":"https://example.test/v1","apiKey":"test",
                "models":[{"upstreamModelId":"model"}],
                "bindings":[{"agentType":"codex","enabled":true},{"agentType":"claude","enabled":true}]
            })).unwrap()["id"].as_str().unwrap().to_string();
            let c = database::open(&paths).unwrap();
            let model = c.query_row("SELECT id FROM service_provider_models WHERE provider_id=?1",
                [&provider], |r| r.get::<_,String>(0)).unwrap();
            Self { paths, root, provider, model }
        }
        fn request(&self, id: &str, agent: &str, created_at: i64, input: Option<i64>, output: Option<i64>) {
            let c = database::open(&self.paths).unwrap();
            c.execute("INSERT INTO service_gateway_requests(id,agent_type,generation,protocol_in,request_path,state,created_at,finished_at)
                VALUES(?1,?2,0,'responses','/responses','completed',?3,?3+1)", params![id,agent,created_at]).unwrap();
            c.execute("INSERT INTO service_gateway_attempts(id,request_id,attempt_no,provider_id,provider_name,credential_id,
                model_id,upstream_model,protocol_out,state,started_at,finished_at,input_tokens,output_tokens,cached_input_tokens)
                VALUES(?1,?1,1,?2,'共享服务商','key',?3,'model','responses','succeeded',?4,?4+1,?5,?6,?7)",
                params![id,self.provider,self.model,created_at,input,output,input.map(|_|10)]).unwrap();
        }
        fn stats(&self, filters: Value) -> Value { super::super::get_stats_data(&self.paths, filters).unwrap() }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if self.root.parent() == Some(std::env::temp_dir().as_path())
                && self.root.file_name().unwrap().to_string_lossy().starts_with("ai-manager-service-usage-") {
                let _ = std::fs::remove_dir_all(&self.root);
            }
        }
    }

    #[test]
    fn service_usage_refresh_skips_local_logs_and_preserves_history_until_rollback() {
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(async {
            let f = Fixture::new();
            let source = f.root.join("indexed.jsonl");
            let raw_path = source.to_string_lossy().to_string();
            let session = concat!(
                "{\"timestamp\":\"2026-11-16T01:00:00Z\",\"payload\":{\"type\":\"session_meta\",\"model\":\"fresh-model\"}}\n",
                "{\"timestamp\":\"2026-11-16T01:00:01Z\",\"payload\":{\"type\":\"token_count\",\"info\":{\"total_token_usage\":{\"input_tokens\":30,\"output_tokens\":10}}}}\n"
            );
            std::fs::write(&source, session).unwrap();
            std::fs::write(f.root.join("new.jsonl"), session.replace("fresh-model","new-model")).unwrap();
            usage_store::replace_sessions(&f.paths, &[UsageSessionUpdate {
                raw_path:raw_path.clone(), app_type:"codex".into(), updated_at:1,
                logs:vec![json!({"requestId":"historical","appType":"codex","providerId":"legacy",
                    "providerName":"历史服务商","model":"historical-model","createdAt":1,
                    "inputTokens":10,"outputTokens":2,"cacheReadTokens":0,"cacheCreationTokens":0,
                    "rawPath":raw_path,"totalCostUsd":1.0,"costLockedAt":1})], records:vec![]
            }]).unwrap();
            let original = usage_store::read_all_logs(&f.paths).unwrap();
            let revision = usage_store::revision(&f.paths).unwrap();
            let versions = usage_store::read_session_versions(&f.paths).unwrap();
            let state = json!({"providers":[],"skills":[],"sessions":[{"id":"indexed","cli":"codex","rawPath":raw_path}],
                "cliTargets":[{"id":"codex","sessionsPath":f.root}]});
            service_provider::switch_backend(&f.paths,"service").unwrap();
            f.request("direct", "codex", 100, Some(100), Some(20));
            for _ in 0..2 {
                assert!(super::super::sync_pending_usage(&f.paths,&state).await.unwrap().is_empty());
                let stats = super::super::sync_usage(&f.paths,json!({}),&state).await.unwrap();
                assert_eq!(stats["data"]["summary"]["requestCount"],2);
                assert_eq!(stats["data"]["summary"]["actualTokens"],132);
                assert_eq!(stats["data"]["logTotalCount"],2);
                let skill_stats = super::super::get_skill_usage_stats(&f.paths,json!({}),&state).await.unwrap();
                assert_eq!(skill_stats["data"]["summary"]["usageCount"],0);
            }
            assert_eq!(usage_store::read_all_logs(&f.paths).unwrap(),original);
            assert_eq!(usage_store::revision(&f.paths).unwrap(),revision);
            assert_eq!(usage_store::read_session_versions(&f.paths).unwrap(),versions);
            assert_eq!(usage_store::read_session_skill_versions(&f.paths).unwrap()[&raw_path].1,0);
            let exported = f.stats(json!({"includeAllLogs":true}));
            assert_eq!(exported["logs"].as_array().unwrap().len(),2);
            assert_eq!(f.stats(json!({"providerId":"legacy"}))["summary"]["requestCount"],1);
            // 损坏/不可读取的日志同样不会影响新模式刷新。
            std::fs::write(f.root.join("invalid.jsonl"), "not-json").unwrap();
            assert!(super::super::sync_pending_usage(&f.paths,&state).await.unwrap().is_empty());
            std::fs::remove_file(f.root.join("invalid.jsonl")).unwrap();
            service_provider::switch_backend(&f.paths,"legacy").unwrap();
            super::super::sync_pending_usage(&f.paths,&state).await.unwrap();
            let scanned = usage_store::read_all_logs(&f.paths).unwrap();
            assert!(scanned.iter().any(|log|log["model"]=="fresh-model"));
            assert!(scanned.iter().any(|log|log["model"]=="new-model"));
            assert_eq!(f.stats(json!({"requestSource":"service-gateway"}))["summary"]["requestCount"],1);
        });
    }

    #[test]
    fn service_skill_events_deduplicate_replays_and_keep_pre_gateway_history() {
        use super::super::gateway_skills;
        let f = Fixture::new();
        service_provider::switch_backend(&f.paths,"service").unwrap();
        let c = database::open(&f.paths).unwrap();
        c.execute("UPDATE app_metadata SET value='100' WHERE key='service_gateway:skills_started_at'",[]).unwrap();
        c.execute("UPDATE service_operations SET completed_at=110 WHERE operation_type='switch' AND to_backend='service'",[]).unwrap();
        usage_store::write_session_skills(&f.paths,&[usage_store::SessionSkillUpdate {
            raw_path:"old-session".into(),app_type:"codex".into(),session_id:"old".into(),
            created_at:50,updated_at:250,calls:vec![
                json!({"alias":"before","discoverable":true,"createdAt":50}),
                json!({"alias":"duplicate","discoverable":true,"createdAt":150}),
                json!({"alias":"after","discoverable":true,"createdAt":250})
            ]
        }]).unwrap();
        let payload = json!({"input":[{"type":"function_call","call_id":"read-skill","name":"read",
            "arguments":"{\"path\":\"C:/skills/design/SKILL.md\"}"}]});
        for (id,session,time) in [("one","session-one",150),("retry","session-one",160),("two","session-two",170)] {
            f.request(id,"codex",time,Some(100),Some(20));
            c.execute("UPDATE service_gateway_requests SET instance_id='same-instance' WHERE id=?1",[id]).unwrap();
            let record = json!({"serviceRequestId":id,"instanceId":"same-instance"});
            let mut request_payload = payload.clone();
            let header = if id == "retry" {
                request_payload["metadata"] = json!({"user_id":"user_test_account_test_session_session-one"});
                ""
            } else { session };
            gateway_skills::record_request(&f.paths,&record,&request_payload,header).unwrap();
            gateway_skills::record_request(&f.paths,&record,&request_payload,header).unwrap();
        }
        let calls = gateway_skills::read_calls(&f.paths).unwrap();
        assert_eq!(calls.len(),2);
        assert_eq!(calls.iter().map(|call|call["requestIds"].as_array().unwrap().len()).sum::<usize>(),3);
        assert!(calls.iter().all(|call|call["alias"]=="design"));
        assert!(!json!(store::query(&c,"SELECT * FROM service_gateway_skill_events",&[]).unwrap())
            .to_string().contains("C:/skills"));
        service_provider::switch_backend(&f.paths,"legacy").unwrap();
        c.execute("UPDATE service_operations SET completed_at=200 WHERE operation_type='switch' AND to_backend='legacy'",[]).unwrap();
        let retained = gateway_skills::read_legacy_calls(&f.paths).unwrap();
        assert_eq!(retained.iter().map(|call|store::text(call,"alias")).collect::<Vec<_>>(),["before","after"]);
        assert_eq!(usage_store::read_session_skill_calls(&f.paths,"all",0,0).unwrap().len(),3);
    }

    #[test]
    fn service_usage_preserves_history_filters_and_rolls_back_without_double_counting() {
        let f = Fixture::new();
        service_provider::switch_backend(&f.paths, "service").unwrap();
        let directory = f.root.join("codex");
        let instance = service_provider::activate(&f.paths, &json!([]), &json!({
            "agentType":"codex","providerId":f.provider,"modelId":f.model,"configPath":directory
        })).unwrap();
        let raw_path = directory.join("sessions/session.jsonl").to_string_lossy().to_string();
        let unrelated = f.root.join("other/session.jsonl").to_string_lossy().to_string();
        let log = |id: &str, time: u64, path: &str| json!({
            "requestId":id,"appType":"codex","providerId":"legacy","providerName":"历史服务商",
            "model":"old-model","createdAt":time,"inputTokens":10,"outputTokens":2,"cacheReadTokens":0,
            "cacheCreationTokens":0,"rawPath":path,"totalCostUsd":1.0,"costLockedAt":1
        });
        usage_store::replace_sessions(&f.paths, &[
            UsageSessionUpdate { raw_path:raw_path.clone(),app_type:"codex".into(),updated_at:1,
                logs:vec![log("before",90,&raw_path),log("duplicate",150,&raw_path),log("after",210,&raw_path)],records:vec![] },
            UsageSessionUpdate { raw_path:unrelated.clone(),app_type:"codex".into(),updated_at:1,
                logs:vec![log("unmanaged",150,&unrelated)],records:vec![] }
        ]).unwrap();
        let original = usage_store::read_all_logs(&f.paths).unwrap();
        let c = database::open(&f.paths).unwrap();
        c.execute("UPDATE service_managed_files SET created_at=100", []).unwrap();
        f.request("gateway", "codex", 150, Some(100), Some(20));
        let stats = f.stats(json!({}));
        assert_eq!(stats["summary"]["requestCount"], 3);
        assert_eq!(stats["summary"]["actualTokens"], 144);
        assert_eq!(f.stats(json!({"requestSource":"service-gateway"}))["summary"]["requestCount"], 1);
        assert_eq!(f.stats(json!({"providerId":"legacy","endAt":99}))["summary"]["requestCount"], 1);
        service_provider::detach(&f.paths, store::text(&instance,"instanceId")).unwrap();
        c.execute("UPDATE service_managed_files SET updated_at=200", []).unwrap();
        service_provider::switch_backend(&f.paths, "legacy").unwrap();
        let restored = f.stats(json!({}));
        assert_eq!(restored["summary"]["requestCount"], 4);
        assert_eq!(restored["summary"]["actualTokens"], 156);
        assert_eq!(usage_store::read_all_logs(&f.paths).unwrap(), original);
        assert_eq!(f.stats(json!({}))["summary"], restored["summary"]);
    }

    #[test]
    fn service_usage_retains_null_and_refreshes_provider_cache_when_ledger_changes() {
        let f = Fixture::new();
        f.request("unknown", "codex", 100, None, None);
        let filter = json!({"statsScope":"provider","appType":"codex","providerId":f.provider});
        let first = f.stats(filter.clone());
        assert_eq!(first["summary"]["requestCount"],1);
        assert!(first["summary"]["actualTokens"].is_null());
        assert!(first["summary"]["totalCostUsd"].is_null());
        let c = database::open(&f.paths).unwrap();
        c.execute("UPDATE service_gateway_attempts SET input_tokens=100,output_tokens=20,cached_input_tokens=10,
            estimated_cost_micros=2000000 WHERE id='unknown'", []).unwrap();
        let next = f.stats(filter.clone());
        assert_eq!(next["summary"]["actualTokens"],120);
        assert_eq!(next["summary"]["inputTokens"],90);
        assert_eq!(next["summary"]["totalCostUsd"],2.0);
        c.execute("UPDATE service_gateway_attempts SET estimated_cost_micros=3000000 WHERE id='unknown'", []).unwrap();
        assert_eq!(f.stats(filter)["summary"]["totalCostUsd"],3.0);
    }

    #[test]
    fn service_usage_deduplicates_claude_responses_and_desktop_turns() {
        let f = Fixture::new();
        f.request("claude-request", "claude", 100, Some(100), Some(20));
        f.request("desktop-request", "claude-desktop", 100, Some(100), Some(20));
        let mut desktop = vec![json!({"desktopResponseIds":["msg_aim_desktoprequest"]})];
        mark_desktop_logs(&f.paths, &mut desktop).unwrap();
        assert_eq!(desktop[0]["dataSource"], "service-gateway-session");
        let legacy = vec![
            json!({"appType":"claude","requestId":"session:msg_aim_clauderequest"}),
            json!({"appType":"claude-desktop","requestId":"turn","dataSource":desktop[0]["dataSource"]}),
            json!({"appType":"claude","requestId":"session:independent","inputTokens":5,"outputTokens":1})
        ];
        let merged = read(&f.paths).unwrap().merge(legacy);
        assert_eq!(merged.len(),3);
        let claude = merged.iter().find(|log| log["requestId"] == "service:claude-request").unwrap();
        assert_eq!(claude["inputTokens"],90);
        assert_eq!(claude["actualTokens"],120);
    }

    #[test]
    fn calendar_usage_deduplicates_gateway_responses_across_range_boundaries() {
        let f = Fixture::new();
        f.request("inside", "claude", 150, Some(100), Some(20));
        f.request("before-range", "claude", 99, Some(100), Some(20));
        f.request("after-range", "claude", 201, Some(100), Some(20));
        let c = database::open(&f.paths).unwrap();
        c.execute("UPDATE service_gateway_attempts SET upstream_response_id='msg_upstream'
            WHERE id='after-range'", []).unwrap();
        let logs = [
            "session:msg_aim_beforerange",
            "session:msg_upstream",
            "session:msg_independent",
        ].into_iter().map(|id| json!({
            "requestId": id, "createdAt": 150, "appType": "claude",
            "inputTokens": 10, "outputTokens": 2, "cacheReadTokens": 0,
        })).collect();
        usage_store::replace_sessions(&f.paths, &[UsageSessionUpdate {
            raw_path: "calendar.jsonl".into(), app_type: "claude".into(),
            updated_at: 200, logs, records: vec![],
        }]).unwrap();
        let ordinary = f.stats(json!({"startAt": 100, "endAt": 200}));
        let calendar = f.stats(json!({"startAt": 100, "endAt": 200, "statsScope": "calendar"}));
        assert_eq!(calendar["trends"], ordinary["trends"]);
        assert_eq!(calendar["trends"][0]["requestCount"], 2);
        assert_eq!(calendar["trends"][0]["actualTokens"], 132);
    }

    #[test]
    fn service_usage_global_price_snapshot_keeps_original_exchange_rate() {
        let pricing = json!({"exchangeRate":9.0,"items":[]});
        let log = json!({
            "estimatedCostMicros":765,"currency":"CNY","inputTokens":100,"outputTokens":10,"cacheReadTokens":20,
            "servicePricing":{"source":"global","usagePricingSnapshot":{"exchangeRate":7.5}}
        });
        let enriched = enrich(&log, &pricing, &HashMap::new());
        assert!((enriched["totalCostUsd"].as_f64().unwrap() - 0.000102).abs() < 1e-12);
    }

    #[test]
    fn service_usage_keeps_existing_pricing_workflow_and_locks_historical_gateway_costs() {
        let f = Fixture::new();
        f.request("priced", "codex", 100, Some(100), Some(20));
        let pricing = |price: f64| super::super::normalize_pricing_config(json!({
            "exchangeRate":7.2,"items":[{"id":"price","modelId":"model","currency":"USD",
                "inputCostPerMillion":price,"outputCostPerMillion":price,"cacheReadCostPerMillion":price}]
        })).unwrap();
        usage_store::write_pricing(&f.paths, &pricing(100.0)).unwrap();
        assert_eq!(f.stats(json!({}))["summary"]["totalCostUsd"],0.012);
        tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(
            super::super::save_pricing(&f.paths, pricing(200.0))
        ).unwrap();
        assert_eq!(f.stats(json!({}))["summary"]["totalCostUsd"],0.012);
        f.request("later", "codex", 200, Some(100), Some(20));
        assert_eq!(f.stats(json!({"startAt":200}))["summary"]["totalCostUsd"],0.024);
        let c = database::open(&f.paths).unwrap();
        let snapshot = store::query(&c,"SELECT price_snapshot_json AS priceJson FROM service_gateway_attempts WHERE id='priced'",&[]).unwrap();
        assert_eq!(snapshot[0]["price"]["usagePricingSnapshot"]["model"]["inputCostPerMillion"],100.0);
    }
}

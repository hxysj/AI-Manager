use super::*;
use crate::core::{database, service_provider_store as store};
use rusqlite::params;

struct Event {
    key: String,
    kind: &'static str,
    references: Vec<SkillReference>,
}

fn explicit_references(text: &str) -> Vec<SkillReference> {
    // Agent 会把技能目录、环境说明放进 user 消息；它们不代表实际调用。
    if [
        "<skills_instructions>",
        "## Available skills",
        "### Available skills",
        "# AGENTS.md instructions",
        "<environment_context>",
    ]
    .iter()
    .any(|marker| text.contains(marker))
    {
        return Vec::new();
    }
    let mut result = Vec::new();
    for capture in dollar_skill_regex().captures_iter(text) {
        append_skill_reference(&mut result, &capture[1], true);
    }
    for line in text.lines().map(str::trim) {
        if line.starts_with('/') {
            for capture in slash_skill_regex().captures_iter(line).take(1) {
                append_skill_reference(&mut result, &capture[1], capture[1].contains(':'));
            }
        }
    }
    // Claude Code 的斜杠命令会以结构化标签发送给模型。
    for part in text.split("<command-name>").skip(1) {
        if let Some((command, _)) = part.split_once("</command-name>") {
            if let Some(capture) = slash_skill_regex().captures(command.trim()) {
                append_skill_reference(&mut result, &capture[1], capture[1].contains(':'));
            }
        }
    }
    result.retain(|reference| reference.alias.len() <= 256);
    result
}

fn tool_references(name: &str, arguments: &Value) -> Vec<SkillReference> {
    let arguments = arguments
        .as_str()
        .and_then(|text| serde_json::from_str::<Value>(text).ok())
        .unwrap_or_else(|| arguments.clone());
    let mut result = Vec::new();
    if name.eq_ignore_ascii_case("skill") || name.ends_with("__Skill") {
        let alias = store::text(&arguments, "skill").trim();
        if !alias.is_empty()
            && alias.len() <= 256
            && alias
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || ".-_:".contains(c))
        {
            append_skill_reference(&mut result, alias, true);
        }
    }
    let mut texts = Vec::new();
    collect_text_values(&arguments, &mut texts);
    for text in texts {
        for capture in path_skill_regex().captures_iter(&text) {
            append_skill_reference(&mut result, &capture[1], true);
        }
        for capture in skill_resource_regex().captures_iter(&text) {
            append_skill_reference(&mut result, &capture[1], true);
        }
    }
    result.retain(|reference| reference.alias.len() <= 256);
    result
}

fn append_tool(events: &mut Vec<Event>, tool: &Value, fallback: &str) {
    let function = tool
        .get("function")
        .or_else(|| tool.get("functionCall"))
        .unwrap_or(tool);
    let arguments = function
        .get("arguments")
        .or_else(|| function.get("input"))
        .or_else(|| function.get("args"))
        .unwrap_or(&Value::Null);
    let references = tool_references(store::text(function, "name"), arguments);
    if !references.is_empty() {
        let identity = ["call_id", "id"]
            .iter()
            .find_map(|key| {
                tool[*key]
                    .as_str()
                    .or_else(|| function[*key].as_str())
                    .filter(|id| !id.is_empty())
            })
            .unwrap_or(fallback);
        events.push(Event {
            key: store::hash(&json!(["tool", identity]).to_string()),
            kind: "tool",
            references,
        });
    }
}

fn observe(payload: &Value) -> Vec<Event> {
    let input = payload
        .get("input")
        .or_else(|| payload.get("messages"))
        .or_else(|| payload.get("contents"))
        .cloned()
        .unwrap_or(Value::Null);
    let messages = match input {
        Value::String(text) => vec![json!({"role":"user","content":text})],
        Value::Array(items) => items,
        _ => Vec::new(),
    };
    let mut prefix = String::new();
    let mut events = Vec::new();
    for message in messages {
        let role = store::text(&message, "role");
        if matches!(role, "system" | "developer") {
            continue;
        }
        // 前缀只在无稳定消息/调用 ID 时使用，追加历史不会改变已有事件的身份。
        prefix = store::hash(&json!([prefix, message]).to_string());
        if matches!(
            store::text(&message, "type"),
            "function_call" | "custom_tool_call"
        ) {
            let mut tool = message.clone();
            if tool.get("arguments").is_none() {
                tool["arguments"] = message["input"].clone();
            }
            append_tool(&mut events, &tool, &prefix);
        }
        if matches!(role, "assistant" | "model") {
            for tool in message["tool_calls"].as_array().into_iter().flatten() {
                append_tool(&mut events, tool, &prefix);
            }
            for (index, part) in message
                .get("content")
                .or_else(|| message.get("parts"))
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
            {
                if part["type"] == "tool_use" || part.get("functionCall").is_some() {
                    append_tool(&mut events, part, &format!("{prefix}:{index}"));
                }
            }
        }
        if role == "user" {
            let content = message
                .get("content")
                .or_else(|| message.get("parts"))
                .unwrap_or(&Value::Null);
            let text = if let Some(text) = content.as_str() {
                text.to_string()
            } else {
                content
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|part| {
                        part.get("type").is_none()
                            || matches!(store::text(part, "type"), "text" | "input_text")
                    })
                    .filter_map(|part| part["text"].as_str())
                    .collect::<Vec<_>>()
                    .join("\n")
            };
            let references = explicit_references(&text);
            if !references.is_empty() {
                let identity = message["id"]
                    .as_str()
                    .filter(|id| !id.is_empty())
                    .unwrap_or(&prefix);
                events.push(Event {
                    key: store::hash(&json!(["explicit", identity]).to_string()),
                    kind: "explicit",
                    references,
                });
            }
        }
    }
    events
}

// 在鉴权后的入站请求中采集证据；不读客户端日志、不改变发送内容、不保存正文。
pub(crate) fn record_request(
    paths: &AppPaths,
    record: &Value,
    payload: &Value,
    header_session: &str,
) -> Result<(), ManagerError> {
    let events = observe(payload);
    if events.is_empty()
        && payload["previous_response_id"]
            .as_str()
            .is_none_or(str::is_empty)
    {
        return Ok(());
    }
    let mut c = database::open(paths)?;
    let tx = c.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let request = store::query(
        &tx,
        "SELECT r.created_at,r.agent_type,
        coalesce(nullif(i.config_path,''),r.instance_id,r.id) AS instanceScope
        FROM service_gateway_requests r LEFT JOIN service_agent_instances i ON i.id=r.instance_id
        WHERE r.id=?1",
        &[&store::text(record, "serviceRequestId")],
    )?
    .pop()
    .ok_or_else(|| store::fail("共享网关请求不存在"))?;
    let mut session = if !header_session.is_empty() {
        header_session
    } else {
        ["session_id", "conversation_id"]
            .iter()
            .find_map(|key| {
                payload["metadata"][*key]
                    .as_str()
                    .filter(|id| !id.is_empty())
            })
            .unwrap_or("")
    }
    .to_string();
    if session.is_empty() {
        let user = store::text(&payload["metadata"], "user_id");
        session = serde_json::from_str::<Value>(user)
            .ok()
            .and_then(|user| user["session_id"].as_str().map(str::to_string))
            .or_else(|| user.rsplit_once("_session_").map(|(_, id)| id.to_string()))
            .unwrap_or_default();
    }
    let scope_kind = if !session.is_empty() {
        "session"
    } else if !store::text(record, "instanceId").is_empty() {
        "history"
    } else {
        "request"
    };
    let scope =
        store::hash(&json!([request["agent_type"], request["instanceScope"], session]).to_string());
    let request_id = store::text(record, "serviceRequestId");
    for (index, event) in events.iter().enumerate() {
        for reference in &event.references {
            let alias = reference.alias.to_lowercase();
            let id = store::hash(&json!([scope, event.key, alias]).to_string());
            tx.execute("INSERT INTO service_gateway_skill_events
                (id,scope_hash,scope_kind,event_hash,alias,discoverable,evidence_kind,first_request_id,created_at)
                VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
                ON CONFLICT(scope_hash,event_hash,alias) DO NOTHING",
                params![id,scope,scope_kind,event.key,alias,reference.discoverable,event.kind,
                    request_id,request["created_at"].as_i64()])?;
            // 沿用原 Skill 分析的“最近一次调用之后的请求”口径，不把整个历史
            // 中的每个技能都关联到同一请求；同次事件中的多个技能可共同关联。
            if index + 1 == events.len() {
                tx.execute("INSERT OR IGNORE INTO service_gateway_request_skills(request_id,event_id) VALUES(?1,?2)",
                    params![request_id,id])?;
            }
        }
    }
    if events.is_empty() {
        if let Some(previous) = payload["previous_response_id"]
            .as_str()
            .filter(|id| !id.is_empty())
        {
            // Responses 增量请求没有重发历史时，只沿同一实例的明确 response ID 继承。
            tx.execute(
                "INSERT OR IGNORE INTO service_gateway_request_skills(request_id,event_id)
                SELECT ?1,l.event_id FROM service_gateway_request_skills l WHERE l.request_id=(
                    SELECT a.request_id FROM service_gateway_attempts a
                    JOIN service_gateway_requests r ON r.id=a.request_id
                    WHERE a.upstream_response_id=?2 AND r.instance_id=?3
                    ORDER BY a.finished_at DESC LIMIT 1)",
                params![request_id, previous, store::text(record, "instanceId")],
            )?;
        }
    }
    tx.commit()?;
    Ok(())
}

pub(super) fn read_calls(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    let c = database::open(paths)?;
    if !store::table_exists(&c, "service_gateway_skill_events")? {
        return Ok(Vec::new());
    }
    let mut calls = store::query(&c, "SELECT e.alias,e.discoverable,e.created_at AS createdAt,
        r.agent_type AS cli,
        json_group_array('service:'||l.request_id) FILTER(WHERE l.request_id IS NOT NULL) AS requestIdsJson
        FROM service_gateway_skill_events e JOIN service_gateway_requests r ON r.id=e.first_request_id
        LEFT JOIN service_gateway_request_skills l ON l.event_id=e.id
        GROUP BY e.id ORDER BY e.created_at,e.id", &[])?;
    for call in &mut calls {
        call["discoverable"] = json!(call["discoverable"] == 1);
    }
    Ok(calls)
}

pub(super) fn read_legacy_calls(paths: &AppPaths) -> Result<Vec<Value>, ManagerError> {
    let mut calls = usage_store::read_session_skill_calls(paths, "all", 0, 0)?;
    let c = database::open(paths)?;
    if !store::table_exists(&c, "service_gateway_skill_events")? {
        return Ok(calls);
    }
    let periods = store::query(&c, "SELECT
        max(s.completed_at,CAST(m.value AS INTEGER)) AS startAt,
        (SELECT min(e.completed_at) FROM service_operations e
            WHERE e.operation_type='switch' AND e.to_backend='legacy' AND e.status='completed' AND e.generation>s.generation) AS endAt
        FROM service_operations s JOIN app_metadata m ON m.key='service_gateway:skills_started_at'
        WHERE s.operation_type='switch' AND s.to_backend='service' AND s.status='completed'",
        &[])?;
    calls.retain(|call| {
        !periods.iter().any(|period| {
            let time = number_value(call.get("createdAt"), 0);
            time > 0
                && time >= number_value(period.get("startAt"), 0)
                && (period["endAt"].is_null() || time < number_value(period.get("endAt"), 0))
        })
    });
    Ok(calls)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gateway_skills_only_observe_explicit_references_and_tool_calls() {
        let payload = json!({"messages":[
            {"role":"system","content":"$fake /fake C:/skills/catalog/SKILL.md"},
            {"role":"user","content":"<skills_instructions>### Available skills\n$fake C:/skills/catalog/SKILL.md"},
            {"role":"assistant","content":"建议读取 C:/skills/discussed/SKILL.md"},
            {"role":"user","content":[{"type":"tool_result","content":"$fake C:/skills/body/SKILL.md"}]},
            {"role":"user","content":"请使用 $imagegen"},
            {"role":"assistant","content":[{"type":"tool_use","id":"call-1","name":"Skill","input":{"skill":"plugin:design"}}]},
            {"role":"assistant","tool_calls":[{"id":"call-2","function":{"name":"read_file","arguments":"{\"path\":\"C:/skills/frontend-design/SKILL.md\"}"}}]}
        ]});
        let events = observe(&payload);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].references[0].alias, "imagegen");
        assert_eq!(events[1].references[0].alias, "plugin:design");
        assert_eq!(events[2].references[0].alias, "frontend-design");
        let responses = observe(
            &json!({"input":[{"type":"custom_tool_call","call_id":"call-2",
            "input":"cat C:/skills/frontend-design/SKILL.md"}]}),
        );
        assert_eq!(responses[0].key, events[2].key);
        let gemini = observe(
            &json!({"contents":[{"role":"model","parts":[{"functionCall":{
            "id":"call-3","name":"read","args":{"resource":"skill://tools/browser"}}}]}]}),
        );
        assert_eq!(gemini[0].references[0].alias, "browser");
    }

    #[test]
    fn gateway_skills_history_append_keeps_existing_event_identity() {
        let first = json!({"input":[{"role":"user","content":"$imagegen"}]});
        let mut next = first.clone();
        next["input"]
            .as_array_mut()
            .unwrap()
            .push(json!({"role":"user","content":"继续"}));
        assert_eq!(observe(&first)[0].key, observe(&next)[0].key);
        next["input"]
            .as_array_mut()
            .unwrap()
            .push(json!({"role":"user","content":"$imagegen"}));
        let events = observe(&next);
        assert_eq!(events.len(), 2);
        assert_ne!(events[0].key, events[1].key);
        let command = observe(&json!({"messages":[{"role":"user",
            "content":"<command-name>/plugin:design</command-name><command-args>page</command-args>"}]}));
        assert_eq!(command[0].references[0].alias, "plugin:design");
    }
}

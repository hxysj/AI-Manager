use crate::core::error::ManagerError;
use serde_json::{json, Value};
use std::path::Path;

pub(crate) const CATALOG_FILE: &str = ".monkey_thief_model_catalog.json";

pub(crate) fn effort(value: &Value) -> Option<String> {
    let effort = value.as_str()?.trim().to_ascii_lowercase();
    matches!(
        effort.as_str(),
        "none" | "auto" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
    )
    .then_some(effort)
}

// 模板只用于生成客户端目录，不能把合成档位当成账号真实声明。
pub(crate) fn reasoning(model_id: &str, catalog: &Value) -> Value {
    let declared = catalog["supported_reasoning_levels"].as_array();
    let legacy_template = catalog["reasoning_source"].is_null()
        && catalog["comp_hash"] == "3000"
        && declared.is_some_and(|levels| {
            levels == &vec![
                json!({"effort":"low","description":"低"}),
                json!({"effort":"medium","description":"中"}),
                json!({"effort":"high","description":"高"}),
                json!({"effort":"xhigh","description":"极高"}),
            ]
        });
    let mut levels = Vec::new();
    let mut source = "";
    if let Some(declared) = declared.filter(|_| catalog["reasoning_source"] != "template" && !legacy_template) {
        source = if catalog["reasoning_source"] == "embedded" { "embedded" } else { "catalog" };
        for item in declared {
            if let Some(level) = effort(if item.is_string() { item } else { &item["effort"] }) {
                if !levels.contains(&level) {
                    levels.push(level);
                }
            }
        }
        if declared.is_empty() {
            return json!({"state":"unsupported","levels":[],"source":source});
        }
    }
    let mut default = effort(&catalog["default_reasoning_level"]);
    if levels.is_empty() {
        let (built_in, built_in_default): (&[&str], &str) = match model_id.trim().to_ascii_lowercase().as_str() {
            "gpt-6.2-sol" | "gpt-6.2-astra" | "gpt-6.1-sol" | "gpt-5.6-sol" =>
                (&["low", "medium", "high", "xhigh", "max", "ultra"], "low"),
            "gpt-6.1-terra" | "gpt-6-sol" | "gpt-6-astra" | "gpt-5.6-terra" =>
                (&["low", "medium", "high", "xhigh", "max", "ultra"], "medium"),
            "gpt-6-luna" | "gpt-5.6-luna" | "codex-auto-review" =>
                (&["low", "medium", "high", "xhigh", "max"], "medium"),
            "grok-4.7" | "grok-4.7-latest" => (&["low", "medium", "high", "xhigh"], "high"),
            "deepseek-v4-flash" | "deepseek-v4-pro" | "deepseek-reasoner" =>
                (&["low", "high", "max"], "max"),
            "claude-sonnet-4-6" | "claude-opus-4-6" => (&["low", "medium", "high", "max"], "medium"),
            "claude-opus-4-7" | "claude-opus-4-8" => (&["low", "medium", "high", "xhigh", "max"], "medium"),
            _ => return json!({"state":"unknown","levels":null,"source":"unknown"}),
        };
        source = "embedded";
        levels = built_in.iter().map(|level| (*level).to_owned()).collect();
        default = Some(built_in_default.to_owned());
    }
    let default = default.filter(|value| levels.contains(value)).unwrap_or_else(|| levels[0].clone());
    let mut capabilities = json!({"state":"known","levels":levels,"defaultEffort":default,"source":source});
    if catalog["thinking"].is_object() {
        capabilities["thinking"] = catalog["thinking"].clone();
    }
    capabilities
}

pub(crate) fn reasoning_overrides(capability: &Value, overrides: &Value) -> Result<Value, ManagerError> {
    let invalid = || ManagerError::System("思考档位必须从模型声明的可选范围中选择，默认档位须在当前选项中".into());
    let Some(fields) = overrides.as_object() else { return Err(invalid()) };
    if fields.keys().any(|key| !["allowedEfforts", "defaultEffort"].contains(&key.as_str())) {
        return Err(invalid());
    }
    let mut result = capability.clone();
    if let Some(allowed) = fields.get("allowedEfforts") {
        let allowed = allowed.as_array().ok_or_else(invalid)?;
        if !allowed.is_empty() {
            let supported = capability["levels"].as_array().ok_or_else(invalid)?;
            let mut levels = Vec::new();
            for value in allowed {
                let value = json!(effort(value).ok_or_else(invalid)?);
                if !supported.contains(&value) {
                    return Err(invalid());
                }
                if !levels.contains(&value) {
                    levels.push(value);
                }
            }
            result["levels"] = json!(levels);
        }
    }
    if let Some(value) = fields.get("defaultEffort").filter(|value| !value.is_null()) {
        let value = json!(effort(value).ok_or_else(invalid)?);
        if !result["levels"].as_array().is_some_and(|levels| levels.contains(&value)) {
            return Err(invalid());
        }
        result["defaultEffort"] = value;
    } else if let Some(levels) = result["levels"].as_array().filter(|levels| !levels.is_empty()) {
        if !levels.contains(&result["defaultEffort"]) {
            result["defaultEffort"] = levels[0].clone();
        }
    }
    Ok(result)
}

pub(crate) fn effective_reasoning(model: &str, catalog: &Value, overrides: &Value) -> Result<Value, ManagerError> {
    let capability = reasoning(model, catalog);
    if overrides.is_null() {
        Ok(capability)
    } else {
        reasoning_overrides(&capability, overrides)
    }
}

pub(crate) fn project_subagent_defaults(content: &str, options: &Value) -> Result<String, ManagerError> {
    let fields = [
        ("default_subagent_model", "defaultSubagentModel"),
        ("default_subagent_reasoning_effort", "defaultSubagentReasoningEffort"),
    ];
    if !fields.iter().any(|(_, option)| options.get(option).is_some()) {
        return Ok(content.to_owned());
    }
    let mut doc = content.parse::<toml_edit::DocumentMut>()
        .map_err(|_| ManagerError::System("Codex 配置不是有效 TOML，无法保存子 Agent 默认值".into()))?;
    if doc.get("agents").is_none() && fields.iter().any(|(_, option)| options[*option].is_string()) {
        doc["agents"] = toml_edit::Item::Table(toml_edit::Table::new());
    }
    if let Some(agents) = doc.get_mut("agents") {
        let agents = agents.as_table_like_mut()
            .ok_or_else(|| ManagerError::System("Codex 配置中的 agents 必须为表".into()))?;
        for (key, option) in fields {
            if let Some(value) = options.get(option) {
                let desired = value.as_str().map(str::trim).filter(|value| !value.is_empty());
                if agents.get(key).and_then(toml_edit::Item::as_str) == desired && (desired.is_some() || agents.get(key).is_none()) {
                    continue;
                }
                if let Some(value) = desired {
                    if option == "defaultSubagentReasoningEffort" {
                        agents.insert(key, toml_edit::value(value.to_ascii_lowercase()));
                    } else {
                        agents.insert(key, toml_edit::value(value));
                    }
                } else {
                    agents.remove(key);
                }
            }
        }
    }
    Ok(doc.to_string())
}

pub(crate) fn project_reasoning(entry: &mut Value, capability: &Value) {
    if capability["state"] == "unknown" {
        return;
    }
    let levels = capability["levels"].as_array().unwrap();
    let projected = levels.iter().map(|effort| {
        let description = entry["supported_reasoning_levels"].as_array().into_iter().flatten()
            .find(|item| item["effort"] == *effort)
            .and_then(|item| item["description"].as_str())
            .unwrap_or(effort.as_str().unwrap_or(""));
        json!({"effort":effort,"description":description})
    }).collect::<Vec<_>>();
    entry["supported_reasoning_levels"] = json!(projected);
    entry["default_reasoning_level"] = capability["defaultEffort"].clone();
    entry["reasoning_source"] = capability["source"].clone();
    if entry.get("multi_agent_reasoning_effort").is_some() && !levels.contains(&entry["multi_agent_reasoning_effort"]) {
        entry.as_object_mut().unwrap().remove("multi_agent_reasoning_effort");
    }
}

pub(crate) fn validate_remote_reasoning(value: &Value) -> Result<(), ManagerError> {
    let items = value.get("models").or_else(|| value.get("data")).unwrap_or(value);
    let mut ids = std::collections::HashSet::new();
    for item in items.as_array().into_iter().flatten() {
        let invalid = || ManagerError::System("远端模型目录的思考档位无效，已保留原目录".into());
        let id = item.get("slug").or_else(|| item.get("id")).and_then(Value::as_str)
            .or_else(|| item.as_str()).unwrap_or("").trim();
        if id.is_empty() || !ids.insert(id.to_ascii_lowercase()) {
            return Err(ManagerError::System("远端模型目录存在空白或重复模型 ID，已保留原目录".into()));
        }
        let Some(declared) = item.get("supported_reasoning_levels") else {
            if !item["default_reasoning_level"].is_null() {
                return Err(invalid());
            }
            continue;
        };
        let mut levels = std::collections::HashSet::new();
        for level in declared.as_array().ok_or_else(invalid)? {
            let level = effort(if level.is_string() { level } else { &level["effort"] })
                .ok_or_else(invalid)?;
            if !levels.insert(level) {
                return Err(invalid());
            }
        }
        if let Some(default) = item.get("default_reasoning_level").filter(|value| !value.is_null()) {
            if !effort(default).is_some_and(|default| levels.contains(&default)) {
                return Err(invalid());
            }
        }
    }
    Ok(())
}

pub(crate) fn project_spawn_tools(payload: &mut Value, user_agent: &str, models: &[Value]) {
    let ua = user_agent.to_ascii_lowercase();
    if !["codex desktop/", "codex-tui/", "codex_cli_rs/", "codex_exec/"]
        .iter().any(|prefix| ua.starts_with(prefix)) && ua != "codex_cli_rs"
    {
        return;
    }
    const MODEL_HEADING: &str = "Available model overrides";
    let mut lines = vec![format!("{MODEL_HEADING} (optional; inherited parent model is preferred):")];
    for model in models {
        let Some(name) = model["name"].as_str().filter(|name| {
            !name.is_empty() && name.chars().all(|ch| ch.is_alphanumeric() || "-_./:@".contains(ch))
        }) else { continue };
        let mut line = format!("- `{name}`");
        let capability = &model["reasoning"];
        if let Some(levels) = capability["levels"].as_array().filter(|levels| !levels.is_empty()) {
            let levels = levels.iter().filter_map(|item| {
                let level = item.as_str()?;
                if matches!(level, "minimal" | "auto") { return None; }
                Some(if *item == capability["defaultEffort"] { format!("{level} (default)") } else { level.to_owned() })
            }).collect::<Vec<_>>();
            line.push_str(&format!(": Reasoning efforts: {}.", levels.join(", ")));
        }
        lines.push(line);
    }
    let mut tools = Vec::new();
    if let Some(payload) = payload.as_object_mut() {
        for (key, value) in payload.iter_mut() {
            let Some(items) = value.as_array_mut() else { continue };
            if key == "tools" {
                tools.push(items);
            } else if key == "input" {
                for item in items.iter_mut().filter(|item| item["type"] == "additional_tools") {
                    if let Some(extra) = item.get_mut("tools").and_then(Value::as_array_mut) {
                        tools.push(extra);
                    }
                }
            }
        }
    }
    fn conflicts(tools: &[Value]) -> bool {
        tools.iter().any(|tool| tool["type"] == "namespace" &&
            (tool["name"].as_str().is_some_and(|name| name.starts_with("collaboration-optimize"))
                || tool["tools"].as_array().is_some_and(|items| conflicts(items))))
    }
    fn update(tools: &mut [Value], lines: &[String]) {
        for tool in tools {
            if tool["type"] == "namespace" {
                if let Some(nested) = tool["tools"].as_array_mut() {
                    update(nested, lines);
                }
            } else if tool["type"] == "function" && tool["name"] == "spawn_agent" {
                let Some(description) = tool["description"].as_str() else { continue };
                let mut retained = Vec::new();
                let mut old_models = false;
                for line in description.split('\n') {
                    if line.trim_start().starts_with(MODEL_HEADING) && line.trim_end().ends_with(':') {
                        old_models = true;
                        continue;
                    }
                    if old_models && line.trim_start().starts_with("- `") { continue; }
                    old_models = false;
                    retained.push(line);
                }
                if lines.len() > 1 {
                    let position = retained.iter().position(|line| line.trim_start().starts_with("Spawns an agent"))
                        .unwrap_or_else(|| retained.iter().rposition(|line| !line.is_empty()).map_or(0, |last| last + 1));
                    retained.splice(position..position, lines.iter().map(String::as_str));
                }
                tool["description"] = json!(retained.join("\n"));
            }
        }
    }
    if tools.iter().any(|entries| conflicts(entries)) {
        return;
    }
    for entries in tools {
        update(entries, &lines);
    }
}

pub(crate) fn normalize_catalog(value: &Value) -> Result<Value, ManagerError> {
    let items = value
        .get("models")
        .or_else(|| value.get("data"))
        .unwrap_or(value)
        .as_array()
        .ok_or_else(|| ManagerError::System("模型列表必须是 models、data 或 JSON 数组".into()))?;
    let mut seen = std::collections::HashSet::new();
    let mut models = Vec::new();
    for item in items {
        let slug = item
            .get("slug")
            .or_else(|| item.get("id"))
            .and_then(Value::as_str)
            .or_else(|| item.as_str())
            .unwrap_or("")
            .trim();
        if slug.is_empty() || !seen.insert(slug.to_ascii_lowercase()) {
            continue;
        }
        let mut model = json!({
            "slug": slug,
            "display_name": slug,
            "description": format!("上游模型：{}", slug),
            "default_reasoning_level": "low",
            "supported_reasoning_levels": [
                { "effort": "low", "description": "低" },
                { "effort": "medium", "description": "中" },
                { "effort": "high", "description": "高" },
                { "effort": "xhigh", "description": "极高" }
            ],
            "shell_type": "unified_exec",
            "visibility": "list",
            "supported_in_api": true,
            "priority": models.len(),
            "upgrade": null,
            "base_instructions": "",
            "supports_reasoning_summaries": false,
            "support_verbosity": false,
            "default_verbosity": null,
            "apply_patch_tool_type": "freeform",
            "truncation_policy": { "mode": "tokens", "limit": 10000 },
            "parallel_tool_calls": true,
            "experimental_supported_tools": [],
            "input_modalities": ["text", "image"],
            "context_window": 272000
        });
        if let Some(fields) = item.as_object() {
            model.as_object_mut().unwrap().extend(fields.clone());
        }
        model["slug"] = json!(slug);
        let context = model["context_window"]
            .as_u64()
            .filter(|value| *value > 0)
            .unwrap_or(272000);
        model["context_window"] = json!(context);
        model["auto_compact_token_limit"] = json!(context / 10 * 9 + context % 10 * 9 / 10);
        model["comp_hash"] = json!("3000");
        if !model["multi_agent_version"].is_string() {
            model["multi_agent_version"] = json!("v1");
        }
        let slug_lower = slug.to_ascii_lowercase();
        model["visibility"] = json!(if slug_lower.starts_with("gpt-image")
            || slug_lower.starts_with("dall-e")
            || slug_lower.contains("auto-review")
            || model["visibility"] == "hide"
        {
            "hide"
        } else {
            "list"
        });
        let capability = reasoning(slug, item);
        let levels = model["supported_reasoning_levels"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let levels = levels.iter().filter_map(|level| {
            if let Some(effort) = level.as_str() {
                Some(json!({ "effort": effort, "description": effort }))
            } else if level["effort"].is_string() {
                Some(json!({ "effort": level["effort"], "description": level["description"].as_str().unwrap_or("") }))
            } else {
                None
            }
        }).collect::<Vec<_>>();
        if !levels.is_empty() {
            if !levels
                .iter()
                .any(|level| level["effort"] == model["default_reasoning_level"])
            {
                model["default_reasoning_level"] = levels[0]["effort"].clone();
            }
            model["supported_reasoning_levels"] = json!(levels);
        } else {
            model["supported_reasoning_levels"] = json!([{ "effort": "low", "description": "低" }]);
            model["default_reasoning_level"] = json!("low");
        }
        if capability["state"] == "unknown" {
            model["reasoning_source"] = json!("template");
        } else {
            project_reasoning(&mut model, &capability);
        }
        models.push(model);
    }
    if models.is_empty() {
        return Err(ManagerError::System(
            "上游未返回有效模型，现有模型列表保持不变".into(),
        ));
    }
    Ok(json!({ "models": models }))
}

pub(crate) fn models_url(base_url: &str) -> Result<url::Url, ManagerError> {
    let mut url = url::Url::parse(base_url.trim())
        .map_err(|error| ManagerError::System(error.to_string()))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err(ManagerError::System(
            "上游地址必须是 HTTP 或 HTTPS 地址".into(),
        ));
    }
    let path = url.path().trim_end_matches('/');
    if !path.ends_with("/models") && !path.ends_with(".json") {
        let path = path
            .strip_suffix("/responses/compact")
            .or_else(|| path.strip_suffix("/responses"))
            .or_else(|| path.strip_suffix("/chat/completions"))
            .or_else(|| path.strip_suffix("/messages"))
            .unwrap_or(path);
        url.set_path(&format!(
            "{}/models",
            if path.is_empty() { "/v1" } else { path }
        ));
    }
    Ok(url)
}

pub(crate) async fn write_atomic(path: &Path, content: &str) -> Result<(), ManagerError> {
    let temporary = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    tokio::fs::write(&temporary, content).await?;
    if let Err(error) = tokio::fs::rename(&temporary, path).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_distinguishes_missing_and_unsupported_and_validates_overrides() {
        let normalized = normalize_catalog(&json!(["third-party", {
            "id":"explicit", "supported_reasoning_levels":[" HIGH ","low","high"],
            "default_reasoning_level":"high"
        }, {"slug":"plain","supported_reasoning_levels":[]}])).unwrap();
        let models = normalized["models"].as_array().unwrap();
        assert_eq!(reasoning("third-party", &models[0])["state"], "unknown");
        assert_eq!(reasoning("plain", &models[2])["state"], "unsupported");
        let capability = reasoning("explicit", &models[1]);
        assert_eq!(capability["levels"], json!(["high","low"]));
        let restricted = reasoning_overrides(&capability, &json!({"allowedEfforts":["low"]})).unwrap();
        assert_eq!(restricted["defaultEffort"], "low");
        assert!(reasoning_overrides(&capability, &json!({"allowedEfforts":["max"]})).is_err());
        assert!(reasoning_overrides(&capability, &json!({"allowedEfforts":["low"],"defaultEffort":"high"})).is_err());
        assert_eq!(reasoning("gpt-5.6-sol", &json!({}))["source"], "embedded");
    }

    #[test]
    fn edits_only_the_two_native_agent_defaults() {
        let original = "# 用户配置\nmodel = 'original'\n\n[agents]\nmax_threads = 4 # 保留\n\n[agents.worker]\nconfig_file = 'worker.toml'\n";
        let options = json!({"defaultSubagentModel":"worker-model","defaultSubagentReasoningEffort":" HIGH "});
        let written = project_subagent_defaults(original, &options).unwrap();
        assert!(written.contains("default_subagent_model = \"worker-model\""));
        assert!(written.contains("default_subagent_reasoning_effort = \"high\""));
        assert_eq!(project_subagent_defaults(&written, &options).unwrap(), written);
        let cleared = project_subagent_defaults(&written, &json!({"defaultSubagentModel":null,"defaultSubagentReasoningEffort":null})).unwrap();
        assert_eq!(cleared, original);
        assert!(project_subagent_defaults("agents = []", &options).is_err());
        assert!(project_subagent_defaults("[bad", &options).is_err());
    }

    #[test]
    fn injects_nested_and_additional_spawn_descriptions_without_editing_arguments() {
        let tool = json!({"type":"function","name":"spawn_agent","description":"已有说明\nAvailable model overrides (old):\n- `old`\nSpawns an agent\n",
            "parameters":{"type":"object","properties":{"model":{"type":"string"}}}});
        let original = json!({"tools":[{"type":"namespace","name":"collaboration","tools":[tool.clone()]}],
            "input":[{"type":"function_call","arguments":"{\"model\":\"old\"}"},
                {"type":"additional_tools","tools":[tool.clone()]}]});
        let models = [json!({"name":"authorized","reasoning":{"levels":["low","high"],"defaultEffort":"high"}})];
        let mut payload = original.clone();
        project_spawn_tools(&mut payload, "non-codex", &models);
        assert_eq!(payload, original);
        project_spawn_tools(&mut payload, "codex_cli_rs/1.0", &models);
        for projected in [&payload["tools"][0]["tools"][0], &payload["input"][1]["tools"][0]] {
            let description = projected["description"].as_str().unwrap();
            assert!(description.contains("`authorized`: Reasoning efforts: low, high (default)."));
            assert!(!description.contains("`old`"));
            assert!(description.ends_with("Spawns an agent\n"));
            assert_eq!(projected["parameters"], tool["parameters"]);
        }
        assert_eq!(payload["input"][0], original["input"][0]);
        let projected = payload.clone();
        project_spawn_tools(&mut payload, "codex_cli_rs/1.0", &models);
        assert_eq!(payload, projected);
        let mut conflict = original.clone();
        conflict["tools"][0]["name"] = json!("collaboration-optimize-compat");
        let unchanged = conflict.clone();
        project_spawn_tools(&mut conflict, "codex_cli_rs/1.0", &models);
        assert_eq!(conflict, unchanged);
    }

    #[test]
    fn normalizes_openai_and_codex_models_without_losing_metadata() {
        let catalog = normalize_catalog(&json!({ "models": [
            { "slug": "custom", "context_window": 100001, "comp_hash": "old", "multi_agent_version": "v2", "custom_capability": true,
              "supported_reasoning_levels": ["medium", "high"], "default_reasoning_level": "missing" },
            { "slug": "CUSTOM" }, { "slug": "gpt-image-1" }, { "slug": "codex-auto-review" }
        ] })).unwrap();
        assert_eq!(catalog["models"].as_array().unwrap().len(), 3);
        let model = &catalog["models"][0];
        assert_eq!(model["auto_compact_token_limit"], 90000);
        assert_eq!(model["comp_hash"], "3000");
        assert_eq!(model["multi_agent_version"], "v2");
        assert_eq!(model["custom_capability"], true);
        assert_eq!(model["supported_reasoning_levels"][0]["effort"], "medium");
        assert_eq!(model["default_reasoning_level"], "medium");
        assert_eq!(catalog["models"][1]["visibility"], "hide");
        assert_eq!(catalog["models"][2]["visibility"], "hide");
        assert_eq!(
            normalize_catalog(&json!({ "data": [{ "id": "custom" }] })).unwrap()["models"][0]
                ["slug"],
            "custom"
        );
        assert!(normalize_catalog(&json!({ "data": [] })).is_err());
        assert!(normalize_catalog(&json!({ "error": "unauthorized" })).is_err());
    }

    #[test]
    fn builds_models_urls_without_duplicating_versions_or_endpoints() {
        for (base, expected) in [
            ("https://example.com/v1/", "https://example.com/v1/models"),
            ("https://example.com", "https://example.com/v1/models"),
            (
                "https://example.com/v1/responses",
                "https://example.com/v1/models",
            ),
            (
                "https://example.com/v1/messages",
                "https://example.com/v1/models",
            ),
            (
                "https://example.com/v1/models",
                "https://example.com/v1/models",
            ),
            (
                "https://example.com/models.json",
                "https://example.com/models.json",
            ),
        ] {
            assert_eq!(models_url(base).unwrap().as_str(), expected);
        }
        assert!(models_url("file:///models.json").is_err());
    }
}

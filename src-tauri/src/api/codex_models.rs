use crate::core::error::ManagerError;
use serde_json::{json, Value};
use std::path::Path;

pub(crate) const CATALOG_FILE: &str = ".monkey_thief_model_catalog.json";

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

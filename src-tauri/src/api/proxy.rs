use crate::api::{codex_account, runtime_provider};
use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

const PROXY_MANAGED_API_KEY: &str = "PROXY_MANAGED";
const PROXY_PROVIDER_INSTANCE_TOKEN_PREFIX: &str = "AI_MANAGER_PROVIDER:";
const CODEX_ACCOUNT_PREFIX: &str = "account:";
const CODEX_OFFICIAL_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

#[derive(Clone)]
pub struct ProxyServerRegistry {
    inner: Arc<Mutex<HashMap<String, tauri::async_runtime::JoinHandle<()>>>>,
    provider_instance_required: Arc<Mutex<HashMap<String, bool>>>,
}

#[derive(Clone)]
struct ProxyContext {
    paths: AppPaths,
    cli_targets: Value,
    cli: String,
}

#[derive(Clone)]
struct ProxyTarget {
    target_type: String,
    name: String,
    base_url: String,
    proxy: String,
    model: String,
    provider: Option<Value>,
}

struct ForwardResult {
    response: reqwest::Response,
    target: ProxyTarget,
    upstream_url: String,
    latency_ms: u64,
}

impl ProxyServerRegistry {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            provider_instance_required: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn ensure_started(
        &self,
        paths: &AppPaths,
        cli_targets: &Value,
        cli: &str,
    ) -> Result<(), ManagerError> {
        if self.inner.lock().await.contains_key(cli) {
            return Ok(());
        }

        let config = read_proxy_config(paths, cli)?;
        let host = string_value(config.get("host"));
        let port = number_value(config.get("port"), default_port(cli));
        let bind_host = if host.is_empty() {
            "127.0.0.1".to_string()
        } else {
            host
        };
        let addr: SocketAddr = format!("{}:{}", bind_host, port)
            .parse()
            .map_err(|error: std::net::AddrParseError| ManagerError::System(error.to_string()))?;
        let listener = TcpListener::bind(addr).await?;
        let context = ProxyContext {
            paths: paths.clone(),
            cli_targets: cli_targets.clone(),
            cli: cli.to_string(),
        };
        let handle = tauri::async_runtime::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    break;
                };
                let io = TokioIo::new(stream);
                let context = context.clone();

                tauri::async_runtime::spawn(async move {
                    let service = service_fn(move |request| {
                        handle_proxy_request(request, context.clone())
                    });

                    if let Err(error) = http1::Builder::new().serve_connection(io, service).await {
                        eprintln!("{error}");
                    }
                });
            }
        });

        self.inner.lock().await.insert(cli.to_string(), handle);
        Ok(())
    }

    pub async fn stop(&self, cli: &str) {
        if *self
            .provider_instance_required
            .lock()
            .await
            .get(cli)
            .unwrap_or(&false)
        {
            return;
        }

        if let Some(handle) = self.inner.lock().await.remove(cli) {
            handle.abort();
        }
    }

    pub async fn require_provider_instance_server(&self, cli: &str) {
        self.provider_instance_required
            .lock()
            .await
            .insert(cli.to_string(), true);
    }
}

pub async fn start_enabled_servers(
    registry: &ProxyServerRegistry,
    paths: &AppPaths,
    cli_targets: &Value,
) -> Result<(), ManagerError> {
    for cli in ["claude", "codex"] {
        if read_proxy_config(paths, cli)?
            .get("enabled")
            .and_then(Value::as_bool)
            == Some(true)
        {
            registry.ensure_started(paths, cli_targets, cli).await?;
        }
    }

    Ok(())
}

pub fn read_proxy_state(paths: &AppPaths, cli: &str) -> Result<Value, ManagerError> {
    let config = read_proxy_config(paths, cli)?;
    let logs = read_logs(paths, cli)?;
    let live_backup = read_live_backup(paths, cli)?;
    let mut state = config;

    state["localBaseUrl"] = json!(if cli == "claude" {
        build_anthropic_local_base_url(&state)
    } else {
        build_local_base_url(&state)
    });
    state["hasLiveBackup"] = json!(!live_backup.is_null());
    state["logs"] = logs;
    Ok(state)
}

pub async fn enable_proxy(
    registry: &ProxyServerRegistry,
    paths: &AppPaths,
    cli_targets: &Value,
    cli: &str,
    payload: Value,
) -> Result<Value, ManagerError> {
    let config = read_proxy_config(paths, cli)?;
    let active_provider_id = get_forward_provider_ids(paths, cli, &config)?
        .into_iter()
        .next()
        .unwrap_or_default();

    if active_provider_id.is_empty() {
        return Err(ManagerError::System(
            "请先把 Provider 加入代理接管列表".to_string(),
        ));
    }

    assert_target_ready(paths, cli_targets, cli, &active_provider_id).await?;
    assert_target_joined(&config, &active_provider_id)?;
    registry.ensure_started(paths, cli_targets, cli).await?;

    let cli_target = runtime_provider::find_cli_target(cli_targets, cli)?;
    let live_config = read_live_config(cli, &cli_target).await?;
    let local_base_url = if cli == "claude" {
        build_anthropic_local_base_url(&config)
    } else {
        build_local_base_url(&config)
    };
    let active_target = get_target(paths, cli, &config, &active_provider_id)?;
    let config_model = if active_target.model.is_empty() {
        read_toml_root_value(&string_value(live_config.get("config")), "model")
    } else {
        active_target.model.clone()
    };
    let next_live_config = if cli == "claude" {
        json!({
          "settings": build_claude_proxy_settings(
            &string_value(live_config.get("settings")),
            &local_base_url,
            active_target.provider.as_ref().unwrap_or(&json!({}))
          )?
        })
    } else {
        let mut auth = live_config
            .get("auth")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        auth.insert("OPENAI_API_KEY".to_string(), json!(PROXY_MANAGED_API_KEY));
        json!({
          "auth": auth,
          "config": set_codex_proxy_config_toml(
            &string_value(live_config.get("config")),
            &local_base_url,
            &config_model
          )
        })
    };
    let live_backup = merge_object(
        live_config.clone(),
        json!({
          "activeProviderId": active_provider_id,
          "previousAccountId": string_value(payload.get("previousAccountId")),
          "previousProfile": payload.get("previousProfile").cloned().unwrap_or(Value::Null),
          "createdAt": now_millis()
        }),
    );

    write_live_backup(paths, cli, &live_backup).await?;
    write_live_config_atomic(cli, &cli_target, &next_live_config).await?;
    write_proxy_config(
        paths,
        cli,
        &normalize_proxy_config(
            &merge_object(
                config,
                json!({
                  "enabled": true,
                  "activeProviderId": active_provider_id,
                  "updatedAt": now_millis()
                }),
            ),
            cli,
        ),
    )
    .await?;
    Ok(read_proxy_state(paths, cli)?)
}

pub async fn disable_proxy(
    registry: &ProxyServerRegistry,
    paths: &AppPaths,
    cli_targets: &Value,
    cli: &str,
) -> Result<Value, ManagerError> {
    let live_backup = read_live_backup(paths, cli)?;

    if live_backup.is_null() {
        return Err(ManagerError::System(format!(
            "{} 代理 Live 备份不存在，无法恢复",
            cli_name(cli)
        )));
    }

    let cli_target = runtime_provider::find_cli_target(cli_targets, cli)?;

    write_live_config_atomic(
        cli,
        &cli_target,
        &json!({
          "auth": live_backup.get("auth").cloned().unwrap_or(Value::Null),
          "config": live_backup.get("config").cloned().unwrap_or_else(|| json!("")),
          "settings": live_backup.get("settings").cloned().unwrap_or_else(|| json!(""))
        }),
    )
    .await?;
    write_live_backup(paths, cli, &Value::Null).await?;

    let config = read_proxy_config(paths, cli)?;
    write_proxy_config(
        paths,
        cli,
        &normalize_proxy_config(
            &merge_object(
                config,
                json!({
                  "enabled": false,
                  "activeProviderId": "",
                  "updatedAt": now_millis()
                }),
            ),
            cli,
        ),
    )
    .await?;
    registry.stop(cli).await;
    Ok(json!({
      "state": read_proxy_state(paths, cli)?,
      "previousAccountId": string_value(live_backup.get("previousAccountId")),
      "previousProfile": live_backup.get("previousProfile").cloned().unwrap_or(Value::Null)
    }))
}

pub async fn add_provider(
    paths: &AppPaths,
    cli_targets: &Value,
    cli: &str,
    payload: Value,
) -> Result<Value, ManagerError> {
    let target_id = target_id_from_payload(payload);
    let config = read_proxy_config(paths, cli)?;

    assert_target_ready(paths, cli_targets, cli, &target_id).await?;

    if !string_array(config.get("failoverProviderIds")).contains(&target_id) {
        let mut ids = string_array(config.get("failoverProviderIds"));

        ids.push(target_id);
        write_proxy_config(
            paths,
            cli,
            &normalize_proxy_config(
                &merge_object(
                    config,
                    json!({
                      "failoverProviderIds": ids,
                      "updatedAt": now_millis()
                    }),
                ),
                cli,
            ),
        )
        .await?;
    }

    Ok(read_proxy_state(paths, cli)?)
}

pub async fn remove_provider(
    paths: &AppPaths,
    cli: &str,
    payload: Value,
) -> Result<Value, ManagerError> {
    let target_id = target_id_from_payload(payload);
    let config = read_proxy_config(paths, cli)?;

    if config.get("enabled").and_then(Value::as_bool) == Some(true)
        && string_value(config.get("activeProviderId")) == target_id
    {
        return Err(ManagerError::System(
            "当前被接管的目标不能移出接管池".to_string(),
        ));
    }

    let next_provider_ids = string_array(config.get("failoverProviderIds"))
        .into_iter()
        .filter(|item| item != &target_id)
        .collect::<Vec<_>>();
    let active_provider_id = if string_value(config.get("activeProviderId")) == target_id {
        next_provider_ids.first().cloned().unwrap_or_default()
    } else {
        string_value(config.get("activeProviderId"))
    };

    write_proxy_config(
        paths,
        cli,
        &normalize_proxy_config(
            &merge_object(
                config,
                json!({
                  "activeProviderId": active_provider_id,
                  "failoverProviderIds": next_provider_ids,
                  "updatedAt": now_millis()
                }),
            ),
            cli,
        ),
    )
    .await?;
    Ok(read_proxy_state(paths, cli)?)
}

pub async fn activate_provider(
    paths: &AppPaths,
    cli_targets: &Value,
    cli: &str,
    payload: Value,
) -> Result<Value, ManagerError> {
    let target_id = target_id_from_payload(payload);
    let config = read_proxy_config(paths, cli)?;

    assert_target_ready(paths, cli_targets, cli, &target_id).await?;
    assert_target_joined(&config, &target_id)?;

    let target = get_target(paths, cli, &config, &target_id)?;

    if config.get("enabled").and_then(Value::as_bool) == Some(true) {
        let cli_target = runtime_provider::find_cli_target(cli_targets, cli)?;
        let live_config = read_live_config(cli, &cli_target).await?;

        if cli == "claude" {
            write_live_config_atomic(
                cli,
                &cli_target,
                &json!({
                  "settings": build_claude_proxy_settings(
                    &string_value(live_config.get("settings")),
                    &build_anthropic_local_base_url(&config),
                    target.provider.as_ref().unwrap_or(&json!({}))
                  )?
                }),
            )
            .await?;
        } else {
            let live_backup = read_live_backup(paths, cli)?;
            write_live_config_atomic(
                cli,
                &cli_target,
                &json!({
                  "auth": live_config.get("auth").cloned().unwrap_or(Value::Null),
                  "config": set_codex_proxy_config_toml(
                    &string_value(live_config.get("config")),
                    &build_local_base_url(&config),
                    &if target.model.is_empty() {
                      read_toml_root_value(&string_value(live_backup.get("config")), "model")
                    } else {
                      target.model
                    }
                  )
                }),
            )
            .await?;
        }
    }

    write_proxy_config(
        paths,
        cli,
        &normalize_proxy_config(
            &merge_object(
                config,
                json!({
                  "activeProviderId": target_id,
                  "updatedAt": now_millis()
                }),
            ),
            cli,
        ),
    )
    .await?;
    Ok(read_proxy_state(paths, cli)?)
}

pub async fn update_account_model(
    paths: &AppPaths,
    cli_targets: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let config = read_proxy_config(paths, "codex")?;
    let next_config = normalize_proxy_config(
        &merge_object(
            config.clone(),
            json!({
              "accountModel": string_value(payload.get("accountModel")),
              "updatedAt": now_millis()
            }),
        ),
        "codex",
    );

    write_proxy_config(paths, "codex", &next_config).await?;

    if next_config.get("enabled").and_then(Value::as_bool) == Some(true)
        && is_account_target(&string_value(next_config.get("activeProviderId")))
    {
        let cli_target = runtime_provider::find_cli_target(cli_targets, "codex")?;
        let live_config = read_live_config("codex", &cli_target).await?;
        let live_backup = read_live_backup(paths, "codex")?;
        let model = first_string(
            next_config.get("accountModel"),
            Some(&json!(read_toml_root_value(
                &string_value(live_backup.get("config")),
                "model"
            ))),
            "",
        );

        write_live_config_atomic(
            "codex",
            &cli_target,
            &json!({
              "auth": live_config.get("auth").cloned().unwrap_or(Value::Null),
              "config": set_codex_proxy_config_toml(
                &string_value(live_config.get("config")),
                &build_local_base_url(&next_config),
                &model
              )
            }),
        )
        .await?;
    }

    Ok(read_proxy_state(paths, "codex")?)
}

pub async fn start_provider_instance_server(
    registry: &ProxyServerRegistry,
    paths: &AppPaths,
    cli_targets: &Value,
    cli: &str,
) -> Result<(), ManagerError> {
    registry.require_provider_instance_server(cli).await;
    registry.ensure_started(paths, cli_targets, cli).await
}

pub fn create_provider_instance_token(provider_id: &str) -> String {
    format!("{}{}", PROXY_PROVIDER_INSTANCE_TOKEN_PREFIX, provider_id.trim())
}

async fn handle_proxy_request(
    request: Request<Incoming>,
    context: ProxyContext,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let response = match process_proxy_request(request, context).await {
        Ok(response) => response,
        Err(error) => json_response(
            StatusCode::BAD_GATEWAY,
            json!({
              "error": {
                "message": error.to_string()
              }
            }),
        ),
    };

    Ok(response)
}

async fn process_proxy_request(
    request: Request<Incoming>,
    context: ProxyContext,
) -> Result<Response<Full<Bytes>>, ManagerError> {
    let route = if context.cli == "claude" {
        normalize_anthropic_endpoint(&request.uri().to_string())
    } else {
        normalize_endpoint(&request.uri().to_string())
    };
    let Some(route) = route else {
        return Ok(json_response(
            StatusCode::NOT_FOUND,
            json!({
              "error": {
                "message": format!("{} 代理不支持该请求路径", cli_name(&context.cli))
              }
            }),
        ));
    };

    let config = read_proxy_config(&context.paths, &context.cli)?;
    let instance_provider_id = get_request_instance_provider_id(&request);
    let request_source = if instance_provider_id.is_empty() {
        "proxy-managed"
    } else {
        "provider-instance"
    };

    if config.get("enabled").and_then(Value::as_bool) != Some(true)
        && instance_provider_id.is_empty()
    {
        return Ok(json_response(
            StatusCode::SERVICE_UNAVAILABLE,
            json!({
              "error": {
                "message": format!("{} 代理未开启接管", cli_name(&context.cli))
              }
            }),
        ));
    }

    let (parts, body) = request.into_parts();
    let body_bytes = body
        .collect()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?
        .to_bytes();
    let provider_ids = if instance_provider_id.is_empty() {
        get_forward_provider_ids(&context.paths, &context.cli, &config)?
    } else {
        vec![instance_provider_id.clone()]
    };
    let mut last_error = None;

    for provider_id in provider_ids {
        let target = get_target(&context.paths, &context.cli, &config, &provider_id)?;
        update_active_provider_if_needed(
            &context.paths,
            &context.cli,
            &config,
            &provider_id,
            &instance_provider_id,
        )
        .await?;

        match forward_request(
            &context.paths,
            &context.cli_targets,
            &context.cli,
            &config,
            &parts.method,
            &parts.headers,
            &route,
            body_bytes.clone(),
            &provider_id,
        )
        .await
        {
            Ok(result) => {
                let status = result.response.status();
                let headers = result.response.headers().clone();
                let response_bytes = result
                    .response
                    .bytes()
                    .await
                    .map_err(|error| ManagerError::System(error.to_string()))?;

                if !status.is_success() {
                    let error_text = String::from_utf8_lossy(&response_bytes).to_string();
                    append_log(
                        &context.paths,
                        &context.cli,
                        json!({
                          "providerId": provider_id,
                          "providerName": target.name,
                          "targetType": target.target_type,
                          "method": parts.method.as_str(),
                          "requestUrl": parts.uri.to_string(),
                          "upstreamUrl": result.upstream_url,
                          "endpoint": route["endpoint"],
                          "instanceProviderId": instance_provider_id,
                          "instanceProviderName": if instance_provider_id.is_empty() { "" } else { target.name.as_str() },
                          "requestSource": request_source,
                          "statusCode": status.as_u16(),
                          "ok": false,
                          "latencyMs": result.latency_ms,
                          "errorMessage": "上游返回非 2xx 状态",
                          "upstreamResponseText": error_text
                        }),
                    )
                    .await?;
                    last_error = Some(format!(
                        "上游返回非 2xx 状态：{} {}",
                        status.as_u16(),
                        error_text
                    ));
                    continue;
                }

                append_log(
                    &context.paths,
                    &context.cli,
                    json!({
                      "providerId": provider_id,
                      "providerName": result.target.name,
                      "targetType": result.target.target_type,
                      "method": parts.method.as_str(),
                      "requestUrl": parts.uri.to_string(),
                      "upstreamUrl": result.upstream_url,
                      "endpoint": route["endpoint"],
                      "instanceProviderId": instance_provider_id,
                      "instanceProviderName": if instance_provider_id.is_empty() { "" } else { result.target.name.as_str() },
                      "requestSource": request_source,
                      "statusCode": status.as_u16(),
                      "ok": status.is_success(),
                      "latencyMs": result.latency_ms,
                      "responseSize": response_bytes.len(),
                      "errorMessage": ""
                    }),
                )
                .await?;

                let mut builder = Response::builder().status(status);

                for (key, value) in headers.iter() {
                    if key.as_str().eq_ignore_ascii_case("transfer-encoding") {
                        continue;
                    }
                    builder = builder.header(key, value);
                }

                return builder
                    .body(Full::new(response_bytes))
                    .map_err(|error| ManagerError::System(error.to_string()));
            }
            Err(error) => {
                append_log(
                    &context.paths,
                    &context.cli,
                    json!({
                      "providerId": provider_id,
                      "providerName": target.name,
                      "targetType": target.target_type,
                      "method": parts.method.as_str(),
                      "requestUrl": parts.uri.to_string(),
                      "upstreamUrl": "",
                      "endpoint": route["endpoint"],
                      "instanceProviderId": instance_provider_id,
                      "instanceProviderName": if instance_provider_id.is_empty() { "" } else { target.name.as_str() },
                      "requestSource": request_source,
                      "statusCode": 0,
                      "ok": false,
                      "latencyMs": 0,
                      "errorMessage": error.to_string(),
                      "upstreamResponseText": ""
                    }),
                )
                .await?;
                last_error = Some(error.to_string());
            }
        }
    }

    Err(ManagerError::System(last_error.unwrap_or_else(|| {
        format!("没有可用的 {} 代理 Provider", cli_name(&context.cli))
    })))
}

async fn update_active_provider_if_needed(
    paths: &AppPaths,
    cli: &str,
    config: &Value,
    provider_id: &str,
    instance_provider_id: &str,
) -> Result<(), ManagerError> {
    if !instance_provider_id.is_empty() || string_value(config.get("activeProviderId")) == provider_id {
        return Ok(());
    }

    write_proxy_config(
        paths,
        cli,
        &normalize_proxy_config(
            &merge_object(
                config.clone(),
                json!({
                  "activeProviderId": provider_id,
                  "updatedAt": now_millis()
                }),
            ),
            cli,
        ),
    )
    .await
}

async fn forward_request(
    paths: &AppPaths,
    cli_targets: &Value,
    cli: &str,
    config: &Value,
    method: &hyper::Method,
    headers: &hyper::HeaderMap,
    route: &Value,
    body: Bytes,
    target_id: &str,
) -> Result<ForwardResult, ManagerError> {
    let target = get_target(paths, cli, config, target_id)?;
    let upstream_url = if cli == "claude" {
        build_anthropic_upstream_url(
            &target.base_url,
            &string_value(route.get("endpoint")),
            &string_value(route.get("search")),
        )?
    } else {
        build_upstream_url(
            &target.base_url,
            &string_value(route.get("endpoint")),
            &string_value(route.get("search")),
        )?
    };
    let mut request_body = body.to_vec();
    let model = if target.model.is_empty() && cli == "codex" {
        read_toml_root_value(&string_value(read_live_backup(paths, cli)?.get("config")), "model")
    } else {
        target.model.clone()
    };

    if method != hyper::Method::GET && method != hyper::Method::HEAD && !model.is_empty() {
        let mut payload: Value = serde_json::from_slice(&request_body)?;

        payload["model"] = json!(model);
        request_body = format!("{}\n", serde_json::to_string(&payload)?).into_bytes();
    }

    let started_at = now_millis();
    let mut request = http_client(&target.proxy)?
        .request(
            reqwest::Method::from_bytes(method.as_str().as_bytes())
                .map_err(|error| ManagerError::System(error.to_string()))?,
            &upstream_url,
        )
        .headers(build_forward_headers(paths, cli_targets, cli, headers, target_id).await?);

    if method != hyper::Method::GET && method != hyper::Method::HEAD {
        request = request.body(request_body);
    }

    let response = request
        .send()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok(ForwardResult {
        response,
        target,
        upstream_url,
        latency_ms: now_millis().saturating_sub(started_at),
    })
}

async fn build_forward_headers(
    paths: &AppPaths,
    cli_targets: &Value,
    cli: &str,
    headers: &hyper::HeaderMap,
    target_id: &str,
) -> Result<HeaderMap, ManagerError> {
    let mut next_headers = HeaderMap::new();

    for (key, value) in headers.iter() {
        let name = key.as_str().to_ascii_lowercase();
        if [
            "authorization",
            "x-api-key",
            "host",
            "content-length",
            "accept-encoding",
        ]
        .contains(&name.as_str())
        {
            continue;
        }

        if let Ok(header_name) = HeaderName::from_bytes(key.as_str().as_bytes()) {
            if let Ok(header_value) = HeaderValue::from_bytes(value.as_bytes()) {
                next_headers.insert(header_name, header_value);
            }
        }
    }

    next_headers.insert("accept-encoding", HeaderValue::from_static("identity"));
    let auth = get_target_auth(paths, cli_targets, cli, target_id).await?;
    let token = string_value(auth.get("token"));
    let account_id = string_value(auth.get("accountId"));

    if !account_id.is_empty() {
        next_headers.insert(
            "chatgpt-account-id",
            HeaderValue::from_str(&account_id)
                .map_err(|error| ManagerError::System(error.to_string()))?,
        );
        next_headers.insert(
            "authorization",
            HeaderValue::from_str(&format!("Bearer {}", token))
                .map_err(|error| ManagerError::System(error.to_string()))?,
        );
        return Ok(next_headers);
    }

    if cli == "claude" {
        next_headers.insert(
            "x-api-key",
            HeaderValue::from_str(&token)
                .map_err(|error| ManagerError::System(error.to_string()))?,
        );
        return Ok(next_headers);
    }

    next_headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", token))
            .map_err(|error| ManagerError::System(error.to_string()))?,
    );
    Ok(next_headers)
}

async fn get_target_auth(
    paths: &AppPaths,
    cli_targets: &Value,
    cli: &str,
    target_id: &str,
) -> Result<Value, ManagerError> {
    if is_account_target(target_id) {
        let cli_target = runtime_provider::find_cli_target(cli_targets, "codex")?;
        let auth =
            codex_account::get_proxy_auth(paths, &account_id_from_target(target_id), &cli_target)
                .await?;

        return Ok(json!({
          "token": auth["accessToken"],
          "accountId": auth["accountId"]
        }));
    }

    Ok(json!({
      "token": get_provider_api_key(paths, cli, target_id)?,
      "accountId": ""
    }))
}

async fn assert_target_ready(
    paths: &AppPaths,
    cli_targets: &Value,
    cli: &str,
    target_id: &str,
) -> Result<(), ManagerError> {
    get_target(paths, cli, &read_proxy_config(paths, cli)?, target_id)?;

    if is_account_target(target_id) {
        let cli_target = runtime_provider::find_cli_target(cli_targets, "codex")?;
        codex_account::get_proxy_auth(paths, &account_id_from_target(target_id), &cli_target)
            .await?;
        return Ok(());
    }

    get_provider_api_key(paths, cli, target_id)?;
    Ok(())
}

fn get_target(
    paths: &AppPaths,
    cli: &str,
    config: &Value,
    target_id: &str,
) -> Result<ProxyTarget, ManagerError> {
    if is_account_target(target_id) {
        if cli != "codex" {
            return Err(ManagerError::System(format!(
                "{} 代理不支持官方账号",
                cli_name(cli)
            )));
        }

        let account_id = account_id_from_target(target_id);
        let accounts = runtime_provider::read_array(&paths.storage_files.codex_accounts)?;
        let account = accounts
            .into_iter()
            .find(|account| string_value(account.get("id")) == account_id)
            .ok_or_else(|| ManagerError::System("Codex 官方账号不存在".to_string()))?;

        if account.get("disabled").and_then(Value::as_bool) == Some(true) {
            return Err(ManagerError::System("Codex 官方账号已禁用".to_string()));
        }

        return Ok(ProxyTarget {
            target_type: "account".to_string(),
            name: first_string(account.get("email"), account.get("accountId"), &account_id),
            base_url: CODEX_OFFICIAL_BASE_URL.to_string(),
            proxy: string_value(account.get("proxy")),
            model: string_value(config.get("accountModel")),
            provider: None,
        });
    }

    let provider = runtime_provider::find_provider(paths, target_id)?;

    if string_value(provider.get("cli")) != cli {
        return Err(ManagerError::System(format!(
            "{} Provider 不存在",
            cli_name(cli)
        )));
    }

    if provider.get("enabled").and_then(Value::as_bool) == Some(false) {
        return Err(ManagerError::System(format!(
            "{} Provider 已禁用",
            cli_name(cli)
        )));
    }

    let runtime_config = provider.get("runtimeConfig").cloned().unwrap_or_else(|| json!({}));

    Ok(ProxyTarget {
        target_type: "provider".to_string(),
        name: string_value(provider.get("name")),
        base_url: string_value(provider.get("baseUrl")),
        proxy: string_value(provider.get("proxy")),
        model: string_value(runtime_config.get("mainModel")),
        provider: Some(provider),
    })
}

fn get_provider_api_key(paths: &AppPaths, cli: &str, provider_id: &str) -> Result<String, ManagerError> {
    let api_key = runtime_provider::get_provider_api_key(paths, provider_id)?;

    if api_key.is_empty() {
        return Err(ManagerError::System(format!(
            "当前 {} Provider 缺少 API Key",
            cli_name(cli)
        )));
    }

    Ok(api_key)
}

fn get_forward_provider_ids(
    paths: &AppPaths,
    cli: &str,
    config: &Value,
) -> Result<Vec<String>, ManagerError> {
    let mut ids = Vec::new();
    let active_provider_id = string_value(config.get("activeProviderId"));

    if !active_provider_id.is_empty() {
        ids.push(active_provider_id);
    }

    ids.extend(string_array(config.get("failoverProviderIds")));
    ids.dedup();

    Ok(ids
        .into_iter()
        .filter(|target_id| is_target_enabled(paths, cli, target_id))
        .collect())
}

fn is_target_enabled(paths: &AppPaths, cli: &str, target_id: &str) -> bool {
    if is_account_target(target_id) {
        return runtime_provider::read_array(&paths.storage_files.codex_accounts)
            .map(|accounts| {
                accounts.into_iter().any(|account| {
                    string_value(account.get("id")) == account_id_from_target(target_id)
                        && account.get("disabled").and_then(Value::as_bool) != Some(true)
                })
            })
            .unwrap_or(false);
    }

    runtime_provider::find_provider(paths, target_id)
        .map(|provider| {
            string_value(provider.get("cli")) == cli
                && provider.get("enabled").and_then(Value::as_bool) != Some(false)
        })
        .unwrap_or(false)
}

fn assert_target_joined(config: &Value, target_id: &str) -> Result<(), ManagerError> {
    if !string_array(config.get("failoverProviderIds")).contains(&target_id.to_string()) {
        return Err(ManagerError::System(
            "请先把该目标加入代理接管池".to_string(),
        ));
    }

    Ok(())
}

fn read_proxy_config(paths: &AppPaths, cli: &str) -> Result<Value, ManagerError> {
    let path = proxy_config_path(paths, cli);
    let value = match std::fs::read_to_string(path) {
        Ok(content) => serde_json::from_str(&content)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(error) => return Err(ManagerError::Io(error)),
    };

    Ok(normalize_proxy_config(&value, cli))
}

async fn write_proxy_config(paths: &AppPaths, cli: &str, config: &Value) -> Result<(), ManagerError> {
    runtime_provider::write_json(proxy_config_path(paths, cli), config).await
}

fn read_live_backup(paths: &AppPaths, cli: &str) -> Result<Value, ManagerError> {
    match std::fs::read_to_string(proxy_live_backup_path(paths, cli)) {
        Ok(content) => Ok(serde_json::from_str(&content)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Value::Null),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

async fn write_live_backup(paths: &AppPaths, cli: &str, payload: &Value) -> Result<(), ManagerError> {
    runtime_provider::write_json(proxy_live_backup_path(paths, cli), payload).await
}

fn read_logs(paths: &AppPaths, cli: &str) -> Result<Value, ManagerError> {
    match std::fs::read_to_string(proxy_logs_path(paths, cli)) {
        Ok(content) => Ok(serde_json::from_str(&content)?),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(json!([])),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

async fn append_log(paths: &AppPaths, cli: &str, input: Value) -> Result<(), ManagerError> {
    let mut logs = read_logs(paths, cli)?
        .as_array()
        .cloned()
        .unwrap_or_default();
    let log = merge_object(
        json!({
          "id": format!("proxy-log-{}-{}", now_millis(), uuid::Uuid::new_v4()),
          "appType": cli,
          "dataSource": "proxy",
          "createdAt": now_millis()
        }),
        input,
    );

    logs.insert(0, log);
    logs.truncate(500);
    runtime_provider::write_json(proxy_logs_path(paths, cli), &json!(logs)).await
}

async fn read_live_config(cli: &str, cli_target: &Value) -> Result<Value, ManagerError> {
    let config_path = string_value(cli_target.get("configPath"));

    if config_path.is_empty() {
        return Err(ManagerError::System(format!(
            "{} 配置目录不存在",
            cli_name(cli)
        )));
    }

    if cli == "claude" {
        let settings_path = Path::new(&config_path).join("settings.json");
        let settings = match tokio::fs::read_to_string(settings_path).await {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => "{}\n".to_string(),
            Err(error) => return Err(ManagerError::Io(error)),
        };

        return Ok(json!({ "settings": settings }));
    }

    let auth_path = Path::new(&config_path).join("auth.json");
    let auth_content = tokio::fs::read_to_string(auth_path).await?;
    let auth = serde_json::from_str::<Value>(&auth_content)?;
    let toml_path = Path::new(&config_path).join("config.toml");
    let config = match tokio::fs::read_to_string(toml_path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(ManagerError::Io(error)),
    };

    Ok(json!({
      "auth": auth,
      "config": config
    }))
}

async fn write_live_config_atomic(
    cli: &str,
    cli_target: &Value,
    live_config: &Value,
) -> Result<(), ManagerError> {
    let config_path = string_value(cli_target.get("configPath"));

    if cli == "claude" {
        let settings_path = Path::new(&config_path).join("settings.json");

        if let Some(parent) = settings_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(settings_path, string_value(live_config.get("settings"))).await?;
        return Ok(());
    }

    let auth_path = Path::new(&config_path).join("auth.json");
    let toml_path = Path::new(&config_path).join("config.toml");
    let previous_auth = match tokio::fs::read_to_string(&auth_path).await {
        Ok(content) => Some(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(ManagerError::Io(error)),
    };

    if let Some(parent) = auth_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    tokio::fs::write(
        &auth_path,
        format!(
            "{}\n",
            serde_json::to_string_pretty(live_config.get("auth").unwrap_or(&Value::Null))?
        ),
    )
    .await?;

    if let Err(error) = tokio::fs::write(&toml_path, string_value(live_config.get("config"))).await {
        if let Some(previous_auth) = previous_auth {
            tokio::fs::write(&auth_path, previous_auth).await?;
        } else {
            match tokio::fs::remove_file(&auth_path).await {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(ManagerError::Io(error)),
            }
        }

        return Err(ManagerError::Io(error));
    }

    Ok(())
}

fn normalize_proxy_config(input: &Value, cli: &str) -> Value {
    let host = first_string(input.get("host"), Some(&json!("127.0.0.1")), "127.0.0.1");
    let port = number_value(input.get("port"), default_port(cli));

    json!({
      "enabled": input.get("enabled").and_then(Value::as_bool).unwrap_or(false),
      "host": host,
      "port": port,
      "activeProviderId": string_value(input.get("activeProviderId")),
      "failoverProviderIds": string_array(input.get("failoverProviderIds")),
      "accountModel": string_value(input.get("accountModel")),
      "retryCount": number_value(input.get("retryCount"), 1),
      "streamTimeoutMs": number_value(input.get("streamTimeoutMs"), 120000),
      "requestTimeoutMs": number_value(input.get("requestTimeoutMs"), 120000),
      "updatedAt": number_value(input.get("updatedAt"), 0)
    })
}

fn build_local_base_url(config: &Value) -> String {
    format!(
        "http://{}:{}/v1",
        format_host_for_url(&normalize_host_for_client(&string_value(config.get("host")))),
        number_value(config.get("port"), 15721)
    )
}

fn build_anthropic_local_base_url(config: &Value) -> String {
    format!(
        "http://{}:{}",
        format_host_for_url(&normalize_host_for_client(&string_value(config.get("host")))),
        number_value(config.get("port"), 15722)
    )
}

fn build_upstream_url(base_url: &str, endpoint: &str, search: &str) -> Result<String, ManagerError> {
    let clean_base = base_url.trim().trim_end_matches('/').to_string();

    if clean_base.is_empty() {
        return Err(ManagerError::System(
            "当前 Codex Provider 缺少请求地址".to_string(),
        ));
    }

    let lower_base = clean_base.to_lowercase();

    if lower_base.ends_with("/chat/completions")
        || lower_base.ends_with("/responses")
        || lower_base.ends_with("/responses/compact")
    {
        return Ok(format!("{}{}", clean_base, search));
    }

    let url = url::Url::parse(&clean_base).map_err(|error| ManagerError::System(error.to_string()))?;
    let base_path = url.path().trim_end_matches('/');
    let path_prefix = if !base_path.is_empty() && base_path != "/" {
        ""
    } else {
        "/v1"
    };

    Ok(format!("{}{}{}{}", clean_base, path_prefix, endpoint, search).replace("/v1/v1/", "/v1/"))
}

fn build_anthropic_upstream_url(
    base_url: &str,
    endpoint: &str,
    search: &str,
) -> Result<String, ManagerError> {
    let clean_base = base_url.trim().trim_end_matches('/').to_string();

    if clean_base.is_empty() {
        return Err(ManagerError::System(
            "当前 Claude Provider 缺少请求地址".to_string(),
        ));
    }

    let lower_base = clean_base.to_lowercase();

    if lower_base.ends_with("/messages") || lower_base.ends_with("/messages/count_tokens") {
        return Ok(format!("{}{}", clean_base, search));
    }

    let url = url::Url::parse(&clean_base).map_err(|error| ManagerError::System(error.to_string()))?;
    let base_path = url.path().trim_end_matches('/');
    let path_prefix = if base_path.to_lowercase().ends_with("/v1") {
        ""
    } else {
        "/v1"
    };

    Ok(format!("{}{}{}{}", clean_base, path_prefix, endpoint, search).replace("/v1/v1/", "/v1/"))
}

fn normalize_endpoint(request_url: &str) -> Option<Value> {
    let url = url::Url::parse(&format!("http://127.0.0.1{}", request_url)).ok()?;
    let pathname = regex::Regex::new(r"/+")
        .ok()?
        .replace_all(url.path(), "/")
        .to_string();

    for endpoint in ["/chat/completions", "/responses", "/responses/compact"] {
        if pathname == endpoint
            || pathname == format!("/v1{}", endpoint)
            || pathname == format!("/v1/v1{}", endpoint)
            || pathname == format!("/codex/v1{}", endpoint)
        {
            return Some(json!({
              "endpoint": endpoint,
              "search": url.query().map(|query| format!("?{}", query)).unwrap_or_default()
            }));
        }
    }

    None
}

fn normalize_anthropic_endpoint(request_url: &str) -> Option<Value> {
    let url = url::Url::parse(&format!("http://127.0.0.1{}", request_url)).ok()?;
    let pathname = regex::Regex::new(r"/+")
        .ok()?
        .replace_all(url.path(), "/")
        .to_string();

    for endpoint in ["/messages", "/messages/count_tokens"] {
        if pathname == endpoint || pathname == format!("/v1{}", endpoint) {
            return Some(json!({
              "endpoint": endpoint,
              "search": url.query().map(|query| format!("?{}", query)).unwrap_or_default()
            }));
        }
    }

    None
}

fn build_claude_proxy_settings(
    content: &str,
    local_base_url: &str,
    provider: &Value,
) -> Result<String, ManagerError> {
    let mut settings: Value = if content.trim().is_empty() {
        json!({})
    } else {
        serde_json::from_str(content)?
    };
    let runtime_config = provider.get("runtimeConfig").cloned().unwrap_or_else(|| json!({}));
    let mut env = settings
        .get("env")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    env.insert("ANTHROPIC_AUTH_TOKEN".to_string(), json!(PROXY_MANAGED_API_KEY));
    env.insert("ANTHROPIC_BASE_URL".to_string(), json!(local_base_url));

    for (config_key, env_key) in [
        ("mainModel", "ANTHROPIC_MODEL"),
        ("haikuModel", "ANTHROPIC_DEFAULT_HAIKU_MODEL"),
        ("sonnetModel", "ANTHROPIC_DEFAULT_SONNET_MODEL"),
        ("opusModel", "ANTHROPIC_DEFAULT_OPUS_MODEL"),
    ] {
        let value = string_value(runtime_config.get(config_key));

        if value.is_empty() {
            env.remove(env_key);
        } else {
            env.insert(env_key.to_string(), json!(value));
        }
    }

    settings["env"] = Value::Object(env);
    Ok(format!("{}\n", serde_json::to_string_pretty(&settings)?))
}

fn set_codex_proxy_config_toml(content: &str, local_base_url: &str, model: &str) -> String {
    let model_provider = read_toml_root_value(content, "model_provider");
    let next_content = if model.is_empty() {
        remove_toml_root_value(content, "model")
    } else {
        set_toml_root_value(content, "model", model)
    };

    if model_provider.is_empty() {
        return set_toml_root_value(
            &set_toml_root_value(&next_content, "base_url", local_base_url),
            "wire_api",
            "responses",
        );
    }

    let section_name = format!("model_providers.{}", model_provider);

    set_toml_section_value(
        &set_toml_section_value(&next_content, &section_name, "base_url", local_base_url),
        &section_name,
        "wire_api",
        "responses",
    )
}

fn read_toml_root_value(content: &str, key: &str) -> String {
    for line in content.lines() {
        let text = line.trim();

        if text.is_empty() || text.starts_with('#') || text.starts_with('[') {
            continue;
        }

        let Some((left, right)) = text.split_once('=') else {
            continue;
        };

        if left.trim() == key {
            return parse_toml_string(right);
        }
    }

    String::new()
}

fn parse_toml_string(value: &str) -> String {
    let text = value.trim();

    if text.starts_with('"') && text.ends_with('"') {
        return serde_json::from_str(text).unwrap_or_else(|_| text.trim_matches('"').to_string());
    }

    text.to_string()
}

fn set_toml_root_value(content: &str, key: &str, value: &str) -> String {
    let mut lines = content.lines().map(str::to_string).collect::<Vec<_>>();
    let next_line = format!("{} = {}", key, runtime_provider::to_toml_string(value.to_string()));

    if let Some(index) = lines.iter().position(|line| {
        let text = line.trim();

        !text.is_empty()
            && !text.starts_with('#')
            && !text.starts_with('[')
            && text.split('=').next().map(str::trim) == Some(key)
    }) {
        lines[index] = next_line;
        return format!("{}\n", lines.join("\n").trim_end());
    }

    let insert_index = lines
        .iter()
        .position(|line| line.trim().starts_with('['))
        .unwrap_or(lines.len());

    lines.insert(insert_index, next_line);
    format!("{}\n", lines.join("\n").trim_end())
}

fn remove_toml_root_value(content: &str, key: &str) -> String {
    let mut next_lines = Vec::new();
    let mut in_section = false;

    for line in content.lines() {
        let text = line.trim();

        if text.starts_with('[') {
            in_section = true;
            next_lines.push(line.to_string());
            continue;
        }

        if !in_section
            && text
                .split_once('=')
                .map(|(left, _)| left.trim() == key)
                .unwrap_or(false)
        {
            continue;
        }

        next_lines.push(line.to_string());
    }

    format!("{}\n", next_lines.join("\n").trim_end())
}

fn set_toml_section_value(content: &str, section_name: &str, key: &str, value: &str) -> String {
    let mut lines = content.lines().map(str::to_string).collect::<Vec<_>>();
    let section_header = format!("[{}]", section_name);
    let next_line = format!("{} = {}", key, runtime_provider::to_toml_string(value.to_string()));
    let Some(section_index) = lines.iter().position(|line| line.trim() == section_header) else {
        let trimmed = lines.join("\n").trim_end().to_string();
        let prefix = if trimmed.is_empty() {
            String::new()
        } else {
            format!("{}\n\n", trimmed)
        };

        return format!("{}{}\n{}\n", prefix, section_header, next_line);
    };
    let mut insert_index = section_index + 1;

    while insert_index < lines.len() && !lines[insert_index].trim().starts_with('[') {
        if lines[insert_index]
            .trim()
            .split_once('=')
            .map(|(left, _)| left.trim() == key)
            .unwrap_or(false)
        {
            lines[insert_index] = next_line;
            return format!("{}\n", lines.join("\n").trim_end());
        }

        insert_index += 1;
    }

    lines.insert(insert_index, next_line);
    format!("{}\n", lines.join("\n").trim_end())
}

fn http_client(proxy: &str) -> Result<reqwest::Client, ManagerError> {
    let mut builder = reqwest::Client::builder();
    let proxy = proxy.trim();

    if !proxy.is_empty() {
        builder = builder.proxy(
            reqwest::Proxy::all(proxy).map_err(|error| ManagerError::System(error.to_string()))?,
        );
    }

    builder
        .build()
        .map_err(|error| ManagerError::System(error.to_string()))
}

fn json_response(status: StatusCode, payload: Value) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json; charset=utf-8")
        .body(Full::new(Bytes::from(format!("{}\n", payload))))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::from(format!("{}\n", payload)))))
}

fn get_request_instance_provider_id(request: &Request<Incoming>) -> String {
    let authorization = request
        .headers()
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .trim()
        .to_string();
    let token = regex::Regex::new(r"(?i)^Bearer\s+(.+)$")
        .ok()
        .and_then(|regex| regex.captures(&authorization))
        .and_then(|captures| captures.get(1).map(|value| value.as_str().to_string()))
        .unwrap_or_else(|| {
            request
                .headers()
                .get("x-api-key")
                .and_then(|value| value.to_str().ok())
                .unwrap_or("")
                .trim()
                .to_string()
        });

    provider_id_from_instance_token(&token)
}

fn provider_id_from_instance_token(token: &str) -> String {
    let text = token.trim();

    if !text.starts_with(PROXY_PROVIDER_INSTANCE_TOKEN_PREFIX) {
        return String::new();
    }

    text[PROXY_PROVIDER_INSTANCE_TOKEN_PREFIX.len()..].to_string()
}

fn target_id_from_payload(payload: Value) -> String {
    let account_id = string_value(payload.get("accountId"));

    if !account_id.is_empty() {
        return format!("{}{}", CODEX_ACCOUNT_PREFIX, account_id);
    }

    string_value(payload.get("providerId"))
}

fn is_account_target(target_id: &str) -> bool {
    target_id.starts_with(CODEX_ACCOUNT_PREFIX)
}

fn account_id_from_target(target_id: &str) -> String {
    target_id.trim_start_matches(CODEX_ACCOUNT_PREFIX).to_string()
}

fn proxy_config_path<'a>(paths: &'a AppPaths, cli: &str) -> &'a str {
    if cli == "claude" {
        &paths.storage_files.claude_proxy_config
    } else {
        &paths.storage_files.codex_proxy_config
    }
}

fn proxy_live_backup_path<'a>(paths: &'a AppPaths, cli: &str) -> &'a str {
    if cli == "claude" {
        &paths.storage_files.claude_proxy_live_backup
    } else {
        &paths.storage_files.codex_proxy_live_backup
    }
}

fn proxy_logs_path<'a>(paths: &'a AppPaths, cli: &str) -> &'a str {
    if cli == "claude" {
        &paths.storage_files.claude_proxy_request_logs
    } else {
        &paths.storage_files.codex_proxy_request_logs
    }
}

fn default_port(cli: &str) -> u64 {
    if cli == "claude" {
        15722
    } else {
        15721
    }
}

fn cli_name(cli: &str) -> &'static str {
    if cli == "claude" {
        "Claude"
    } else {
        "Codex"
    }
}

fn normalize_host_for_client(host: &str) -> String {
    match host {
        "0.0.0.0" => "127.0.0.1".to_string(),
        "::" => "::1".to_string(),
        _ => host.to_string(),
    }
}

fn format_host_for_url(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{}]", host)
    } else {
        host.to_string()
    }
}

fn merge_object(left: Value, right: Value) -> Value {
    let mut map = left.as_object().cloned().unwrap_or_default();

    if let Some(right_map) = right.as_object() {
        for (key, value) in right_map {
            map.insert(key.clone(), value.clone());
        }
    }

    Value::Object(map)
}

fn string_array(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|value| string_value(Some(&value)))
        .filter(|value| !value.is_empty())
        .collect()
}

fn number_value(value: Option<&Value>, fallback: u64) -> u64 {
    value.and_then(Value::as_u64).unwrap_or(fallback)
}

fn string_value(value: Option<&Value>) -> String {
    runtime_provider::string_value(value)
}

fn first_string(value: Option<&Value>, fallback: Option<&Value>, default_value: &str) -> String {
    let value = string_value(value);

    if !value.is_empty() {
        return value;
    }

    let fallback = string_value(fallback);

    if !fallback.is_empty() {
        fallback
    } else {
        default_value.to_string()
    }
}

fn now_millis() -> u64 {
    runtime_provider::now_millis()
}

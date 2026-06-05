use crate::api::runtime_provider;
use crate::core::error::ManagerError;
use crate::core::paths::AppPaths;
use base64::Engine;
use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use reqwest::header::{HeaderMap, HeaderValue};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use tauri::Emitter;
use tauri_plugin_opener::OpenerExt;
use tokio::net::TcpListener;
use tokio::sync::Mutex;

const OAUTH_BASE_URL: &str = "https://auth.openai.com";
const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const CODEX_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OAUTH_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const OAUTH_SCOPE: &str = "openid profile email offline_access";
const MISSING_REFRESH_TOKEN_REASON: &str = "missing_refresh_token";

#[derive(Clone)]
pub struct CodexLoginCache {
    inner: Arc<Mutex<CodexLoginInner>>,
}

#[derive(Clone, Debug)]
struct CodexLoginInner {
    state: Option<Value>,
    listener_active: bool,
}

#[derive(Clone)]
struct CallbackContext {
    app: tauri::AppHandle,
    paths: AppPaths,
    cache: CodexLoginCache,
    cli_targets: Value,
}

#[derive(Clone, Debug)]
struct TokenBundle {
    access_token: String,
    refresh_token: String,
    id_token: String,
    expires_at: u64,
    last_refresh: String,
    expired: String,
    token_generation: u64,
    token_updated_at: u64,
}

impl CodexLoginCache {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(CodexLoginInner {
                state: None,
                listener_active: false,
            })),
        }
    }

    pub async fn state(&self) -> Option<Value> {
        self.inner.lock().await.state.clone()
    }

    async fn set_state(&self, state: Option<Value>) {
        self.inner.lock().await.state = state;
    }

    async fn begin_listener(&self) -> bool {
        let mut inner = self.inner.lock().await;

        if inner.listener_active {
            return false;
        }

        inner.listener_active = true;
        true
    }

    async fn finish_listener(&self) {
        self.inner.lock().await.listener_active = false;
    }
}

pub fn read_public_accounts(paths: &AppPaths) -> Result<Value, ManagerError> {
    let accounts = runtime_provider::read_array(&paths.storage_files.codex_accounts)?;
    let active_account_id = read_active_account_id(paths)?;

    Ok(json!(accounts
        .into_iter()
        .map(|account| public_account(&account, &active_account_id, false))
        .collect::<Vec<_>>()))
}

pub async fn start_login(
    app: &tauri::AppHandle,
    paths: &AppPaths,
    login_cache: &CodexLoginCache,
    cli_targets: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    if let Some(state) = login_cache.state().await {
        if state.get("status").and_then(Value::as_str) == Some("pending") {
            return Err(ManagerError::System(
                "Codex 官方登录正在进行中".to_string(),
            ));
        }
    }

    let target_account_id = string_value(payload.get("accountId"));
    if !target_account_id.is_empty()
        && !runtime_provider::read_array(&paths.storage_files.codex_accounts)?
            .iter()
            .any(|account| string_value(account.get("id")) == target_account_id)
    {
        return Err(ManagerError::System("Codex 官方账号不存在".to_string()));
    }

    if !login_cache.begin_listener().await {
        return Err(ManagerError::System(
            "Codex 官方登录正在进行中".to_string(),
        ));
    }

    let verifier = random_url_token(32)?;
    let challenge = base64_url_no_pad(&Sha256::digest(verifier.as_bytes()));
    let state = random_hex(16)?;
    let proxy = string_value(payload.get("proxy"));
    let auth_url = build_auth_url(&challenge, &state)?;
    let login_state = json!({
      "status": "pending",
      "verifier": verifier,
      "state": state,
      "redirectUri": OAUTH_REDIRECT_URI,
      "proxy": proxy,
      "targetAccountId": target_account_id,
      "authUrl": auth_url
    });

    login_cache.set_state(Some(login_state)).await;
    app.emit("state:changed", state_patch(paths, login_cache).await?)
        .map_err(|error| ManagerError::System(error.to_string()))?;

    let context = CallbackContext {
        app: app.clone(),
        paths: paths.clone(),
        cache: login_cache.clone(),
        cli_targets: cli_targets.clone(),
    };

    tauri::async_runtime::spawn(async move {
        if let Err(error) = run_callback_server(context).await {
            eprintln!("{error}");
        }
    });

    app.opener()
        .open_url(auth_url.clone(), None::<&str>)
        .map_err(|error| {
            ManagerError::System(format!("打开 Codex 授权链接失败：{}", error))
        })?;

    Ok(json!({
      "authUrl": auth_url,
      "redirectUri": OAUTH_REDIRECT_URI,
      "status": "pending"
    }))
}

pub async fn cancel_login(
    paths: &AppPaths,
    login_cache: &CodexLoginCache,
) -> Result<Value, ManagerError> {
    let Some(state) = login_cache.state().await else {
        return Ok(state_patch(paths, login_cache).await?);
    };

    if state.get("status").and_then(Value::as_str) != Some("pending") {
        login_cache.set_state(None).await;
        return Ok(state_patch(paths, login_cache).await?);
    }

    let next_state = merge_object(
        state,
        json!({
          "status": "cancelled",
          "message": "Codex 官方登录已取消"
        }),
    );
    login_cache.set_state(Some(next_state)).await;
    login_cache.finish_listener().await;
    login_cache.set_state(None).await;
    Ok(state_patch(paths, login_cache).await?)
}

pub async fn import_auth_json(
    paths: &AppPaths,
    login_cache: &CodexLoginCache,
    cli_targets: &Value,
    payload: Value,
) -> Result<Value, ManagerError> {
    let auth_data: Value = serde_json::from_str(&string_value(payload.get("content")))?;
    let (tokens, mut claims) = create_tokens_from_auth_data(&auth_data)?;

    if tokens.access_token.is_empty() {
        return Err(ManagerError::System(
            "Codex 登录 JSON 数据缺少 access_token".to_string(),
        ));
    }

    let proxy = string_value(payload.get("proxy"));
    let target_account_id = string_value(payload.get("accountId"));
    let accounts = runtime_provider::read_array(&paths.storage_files.codex_accounts)?;
    let target_account = if target_account_id.is_empty() {
        None
    } else {
        accounts
            .iter()
            .find(|account| string_value(account.get("id")) == target_account_id)
    };
    let account_id = extract_account_id(&claims);

    validate_import_target(&accounts, target_account, &target_account_id, &account_id)?;
    let usage = fetch_usage_info(&tokens.access_token, &claims, &proxy).await?;
    let profile = json!({
      "email": extract_email(&claims),
      "sub": string_value(claims.get("sub"))
    });
    let account = save_account(paths, tokens, profile, &mut claims, usage, proxy).await?;
    let active_account_id = read_active_account_id(paths)?;

    if string_value(account.get("id")) == active_account_id {
        write_account_bundle(&account, &find_codex_cli_target(cli_targets)?).await?;
    }

    login_cache.set_state(login_cache.state().await).await;
    Ok(state_patch(paths, login_cache).await?)
}

pub async fn enable_account(
    paths: &AppPaths,
    cli_targets: &Value,
    payload: Value,
) -> Result<(), ManagerError> {
    let account_id = string_value(payload.get("accountId"));
    let cli_target = find_codex_cli_target(cli_targets)?;
    let account = prepare_account_for_switch(paths, &account_id, &cli_target).await?;

    write_account_bundle(&account, &cli_target).await?;
    runtime_provider::write_json(
        &paths.storage_files.codex_active_account_id,
        &json!(string_value(account.get("id"))),
    )
    .await
}

pub async fn clear_account(paths: &AppPaths) -> Result<(), ManagerError> {
    runtime_provider::write_json(&paths.storage_files.codex_active_account_id, &json!("")).await
}

pub async fn refresh_account(
    paths: &AppPaths,
    cli_targets: &Value,
    payload: Value,
) -> Result<(), ManagerError> {
    let account_id = string_value(payload.get("accountId"));
    let sync_auth = payload.get("syncAuth").and_then(Value::as_bool) != Some(false);
    let cli_target = find_codex_cli_target(cli_targets)?;
    let account = refresh_account_usage(paths, &account_id, &cli_target).await?;

    if sync_auth && string_value(account.get("id")) == read_active_account_id(paths)? {
        write_account_bundle(&account, &cli_target).await?;
    }

    Ok(())
}

pub async fn disable_account(paths: &AppPaths, payload: Value) -> Result<(), ManagerError> {
    let account_id = string_value(payload.get("accountId"));
    let mut accounts = runtime_provider::read_array(&paths.storage_files.codex_accounts)?;
    let account = accounts
        .iter()
        .find(|account| string_value(account.get("id")) == account_id)
        .cloned()
        .ok_or_else(|| ManagerError::System("Codex 官方账号不存在".to_string()))?;

    accounts = accounts
        .into_iter()
        .map(|mut item| {
            if string_value(item.get("id")) == string_value(account.get("id")) {
                item["autoRefresh"] = json!(false);
                item["disabled"] = json!(true);
                item["updatedAt"] = json!(now_millis());
            }
            item
        })
        .collect();

    if read_active_account_id(paths)? == string_value(account.get("id")) {
        runtime_provider::write_json(&paths.storage_files.codex_active_account_id, &json!(""))
            .await?;
    }

    runtime_provider::write_json(&paths.storage_files.codex_accounts, &json!(accounts)).await
}

pub async fn restore_account(paths: &AppPaths, payload: Value) -> Result<(), ManagerError> {
    let account_id = string_value(payload.get("accountId"));
    let mut exists = false;
    let accounts = runtime_provider::read_array(&paths.storage_files.codex_accounts)?
        .into_iter()
        .map(|mut account| {
            if string_value(account.get("id")) == account_id {
                exists = true;
                account["disabled"] = json!(false);
                account["updatedAt"] = json!(now_millis());
            }
            account
        })
        .collect::<Vec<_>>();

    if !exists {
        return Err(ManagerError::System("Codex 官方账号不存在".to_string()));
    }

    runtime_provider::write_json(&paths.storage_files.codex_accounts, &json!(accounts)).await
}

pub async fn update_account_proxy(paths: &AppPaths, payload: Value) -> Result<(), ManagerError> {
    let account_id = string_value(payload.get("accountId"));
    let proxy = string_value(payload.get("proxy"));
    let mut exists = false;
    let accounts = runtime_provider::read_array(&paths.storage_files.codex_accounts)?
        .into_iter()
        .map(|mut account| {
            if string_value(account.get("id")) == account_id {
                exists = true;
                if account.get("disabled").and_then(Value::as_bool) == Some(true) {
                    return account;
                }
                account["proxy"] = json!(proxy);
                account["updatedAt"] = json!(now_millis());
            }
            account
        })
        .collect::<Vec<_>>();

    if !exists {
        return Err(ManagerError::System("Codex 官方账号不存在".to_string()));
    }

    if accounts.iter().any(|account| {
        string_value(account.get("id")) == account_id
            && account.get("disabled").and_then(Value::as_bool) == Some(true)
    }) {
        return Err(ManagerError::System(
            "Codex 官方账号已禁用，不能编辑".to_string(),
        ));
    }

    runtime_provider::write_json(&paths.storage_files.codex_accounts, &json!(accounts)).await
}

pub async fn delete_account(
    paths: &AppPaths,
    cli_targets: &Value,
    payload: Value,
) -> Result<(), ManagerError> {
    let account_id = string_value(payload.get("accountId"));
    let accounts = runtime_provider::read_array(&paths.storage_files.codex_accounts)?;
    let account = accounts
        .iter()
        .find(|account| string_value(account.get("id")) == account_id)
        .cloned()
        .ok_or_else(|| ManagerError::System("Codex 官方账号不存在".to_string()))?;

    if account.get("disabled").and_then(Value::as_bool) == Some(true) {
        return Err(ManagerError::System(
            "Codex 官方账号已禁用，不能删除".to_string(),
        ));
    }

    let active_account_id = read_active_account_id(paths)?;
    let next_accounts = accounts
        .into_iter()
        .filter(|account| string_value(account.get("id")) != account_id)
        .collect::<Vec<_>>();

    if active_account_id == account_id {
        runtime_provider::write_json(&paths.storage_files.codex_active_account_id, &json!(""))
            .await?;
        let cli_target = find_codex_cli_target(cli_targets)?;
        let config_path = string_value(cli_target.get("configPath"));

        if !config_path.is_empty() {
            match tokio::fs::remove_file(Path::new(&config_path).join("auth.json")).await {
                Ok(_) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(ManagerError::Io(error)),
            }
        }
    }

    runtime_provider::write_json(&paths.storage_files.codex_accounts, &json!(next_accounts)).await
}

pub async fn account_detail(
    paths: &AppPaths,
    payload: Value,
) -> Result<Value, ManagerError> {
    let account_id = string_value(payload.get("accountId"));
    let active_account_id = read_active_account_id(paths)?;
    let account = runtime_provider::read_array(&paths.storage_files.codex_accounts)?
        .into_iter()
        .find(|account| string_value(account.get("id")) == account_id)
        .ok_or_else(|| ManagerError::System("Codex 官方账号不存在".to_string()))?;

    Ok(json!({
      "status": "ok",
      "data": public_account(&account, &active_account_id, true),
      "message": ""
    }))
}

pub async fn get_proxy_auth(
    paths: &AppPaths,
    account_id: &str,
    cli_target: &Value,
) -> Result<Value, ManagerError> {
    let account = prepare_account_for_switch(paths, account_id, cli_target).await?;
    let access_token = account_access_token(&account);

    if access_token.is_empty() {
        return Err(ManagerError::System(
            "Codex 官方账号缺少 access_token".to_string(),
        ));
    }

    Ok(json!({
      "accessToken": access_token,
      "accountId": first_string(
        account.get("account_id"),
        account.get("accountId"),
        &string_value(account.get("id"))
      ),
      "name": first_string(
        account.get("email"),
        account.get("accountId"),
        &string_value(account.get("id"))
      )
    }))
}

pub async fn state_patch(
    paths: &AppPaths,
    login_cache: &CodexLoginCache,
) -> Result<Value, ManagerError> {
    Ok(json!({
      "codexAccounts": read_public_accounts(paths)?,
      "codexLoginState": public_login_state(login_cache.state().await),
      "refreshedAt": now_millis()
    }))
}

async fn run_callback_server(context: CallbackContext) -> Result<(), ManagerError> {
    let addr: SocketAddr = "127.0.0.1:1455"
        .parse()
        .map_err(|error: std::net::AddrParseError| ManagerError::System(error.to_string()))?;
    let listener = TcpListener::bind(addr).await?;

    let (stream, _) = listener.accept().await?;
    let io = TokioIo::new(stream);
    let callback_context = context.clone();
    let service = service_fn(move |request| handle_callback(request, callback_context.clone()));

    http1::Builder::new()
        .serve_connection(io, service)
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;

    context.cache.finish_listener().await;
    Ok(())
}

async fn handle_callback(
    request: Request<Incoming>,
    context: CallbackContext,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let result = complete_login(request, context.clone()).await;
    let response = match result {
        Ok(message) => text_response(StatusCode::OK, &message),
        Err(error) => {
            let _ = fail_login(&context, error.to_string()).await;
            text_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string())
        }
    };

    Ok(response)
}

async fn complete_login(
    request: Request<Incoming>,
    context: CallbackContext,
) -> Result<String, ManagerError> {
    let url = url::Url::parse(&format!("http://127.0.0.1{}", request.uri()))
        .map_err(|error| ManagerError::System(error.to_string()))?;

    if url.path() != "/auth/callback" {
        return Err(ManagerError::System("Not found".to_string()));
    }

    let code = url.query_pairs().find(|(key, _)| key == "code").map(|(_, value)| value.to_string()).unwrap_or_default();
    let state = url.query_pairs().find(|(key, _)| key == "state").map(|(_, value)| value.to_string()).unwrap_or_default();
    let error = url.query_pairs().find(|(key, _)| key == "error").map(|(_, value)| value.to_string()).unwrap_or_default();

    if !error.is_empty() {
        return Err(ManagerError::System(format!("Codex 登录失败：{}", error)));
    }

    if code.is_empty() {
        return Err(ManagerError::System(
            "Codex 登录回调缺少 authorization code".to_string(),
        ));
    }

    let Some(login_state) = context.cache.state().await else {
        return Err(ManagerError::System(
            "Codex 登录 state 校验失败".to_string(),
        ));
    };

    if string_value(login_state.get("state")) != state {
        return Err(ManagerError::System(
            "Codex 登录 state 校验失败".to_string(),
        ));
    }

    let tokens = exchange_code(&code, &login_state).await?;
    let mut claims = decode_jwt_payload(&first_string(
        Some(&json!(tokens.id_token.clone())),
        Some(&json!(tokens.access_token.clone())),
        "",
    ))?;
    let account_id = extract_account_id(&claims);
    let accounts = runtime_provider::read_array(&context.paths.storage_files.codex_accounts)?;
    let target_account_id = string_value(login_state.get("targetAccountId"));
    let target_account = if target_account_id.is_empty() {
        None
    } else {
        accounts
            .iter()
            .find(|account| string_value(account.get("id")) == target_account_id)
    };

    validate_import_target(&accounts, target_account, &target_account_id, &account_id)?;

    let proxy = string_value(login_state.get("proxy"));
    let usage = fetch_usage_info(&tokens.access_token, &claims, &proxy).await?;
    let profile = json!({
      "email": extract_email(&claims),
      "sub": string_value(claims.get("sub"))
    });
    let account = save_account(&context.paths, tokens, profile, &mut claims, usage, proxy).await?;

    if string_value(account.get("id")) == read_active_account_id(&context.paths)? {
        write_account_bundle(&account, &find_codex_cli_target(&context.cli_targets)?)
        .await?;
    }

    let success_state = merge_object(
        login_state,
        json!({
          "status": "success",
          "message": "Codex 官方登录已完成",
          "account": {
            "id": account["id"],
            "email": account["email"],
            "plan": account["plan"]
          }
        }),
    );
    context.cache.set_state(Some(success_state)).await;
    context.cache.finish_listener().await;
    context
        .app
        .emit("state:changed", state_patch(&context.paths, &context.cache).await?)
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok("Codex 登录已完成，可以返回 Monkey Thief。".to_string())
}

async fn fail_login(context: &CallbackContext, message: String) -> Result<(), ManagerError> {
    if let Some(state) = context.cache.state().await {
        context
            .cache
            .set_state(Some(merge_object(
                state,
                json!({
                  "status": "failed",
                  "message": message
                }),
            )))
            .await;
        context.cache.finish_listener().await;
        context
            .app
            .emit("state:changed", state_patch(&context.paths, &context.cache).await?)
            .map_err(|error| ManagerError::System(error.to_string()))?;
    }

    Ok(())
}

async fn exchange_code(code: &str, login_state: &Value) -> Result<TokenBundle, ManagerError> {
    let redirect_uri = string_value(login_state.get("redirectUri"));
    let verifier = string_value(login_state.get("verifier"));
    let body = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("client_id", CODEX_CLIENT_ID),
        ("redirect_uri", redirect_uri.as_str()),
        ("code_verifier", verifier.as_str()),
    ];
    let response = http_client(&string_value(login_state.get("proxy")))?
        .post(TOKEN_URL)
        .header("accept", "application/json")
        .form(&body)
        .send()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let payload = read_response_json(response).await?;
    let now = now_millis();
    let expires_at = now + payload.get("expires_in").and_then(Value::as_u64).unwrap_or(86400) * 1000;

    Ok(TokenBundle {
        access_token: string_value(payload.get("access_token")),
        refresh_token: string_value(payload.get("refresh_token")),
        id_token: string_value(payload.get("id_token")),
        expires_at,
        last_refresh: format_rfc3339(now),
        expired: format_rfc3339(expires_at),
        token_generation: 0,
        token_updated_at: 0,
    })
}

async fn refresh_token(refresh_token: &str, proxy: &str) -> Result<TokenBundle, ManagerError> {
    let response = http_client(proxy)?
        .post(TOKEN_URL)
        .header("accept", "application/json")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", CODEX_CLIENT_ID),
        ])
        .send()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;

    if !status.is_success() {
        let payload = serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({}));
        let message = first_string(
            payload.get("error_description"),
            payload
                .get("error")
                .and_then(|error| error.get("message"))
                .or_else(|| payload.get("error").and_then(|error| error.get("code")))
                .or_else(|| payload.get("error")),
            &text,
        );
        return Err(ManagerError::System(format!(
            "OpenAI 请求失败：{} {}",
            status.as_u16(),
            message
        )));
    }

    let payload: Value = serde_json::from_str(&text)?;
    let now = now_millis();
    let expires_at = now + payload.get("expires_in").and_then(Value::as_u64).unwrap_or(86400) * 1000;
    let next_refresh_token = first_string(payload.get("refresh_token"), None, refresh_token);

    Ok(TokenBundle {
        access_token: string_value(payload.get("access_token")),
        refresh_token: next_refresh_token,
        id_token: string_value(payload.get("id_token")),
        expires_at,
        last_refresh: format_rfc3339(now),
        expired: format_rfc3339(expires_at),
        token_generation: 0,
        token_updated_at: 0,
    })
}

async fn refresh_account_usage(
    paths: &AppPaths,
    account_id: &str,
    cli_target: &Value,
) -> Result<Value, ManagerError> {
    let accounts = runtime_provider::read_array(&paths.storage_files.codex_accounts)?;
    let account = accounts
        .iter()
        .find(|account| string_value(account.get("id")) == account_id)
        .cloned()
        .ok_or_else(|| ManagerError::System("Codex 官方账号不存在".to_string()))?;

    if account.get("disabled").and_then(Value::as_bool) == Some(true) {
        return Err(ManagerError::System(
            "Codex 官方账号已禁用，不能刷新额度".to_string(),
        ));
    }

    let mut tokens = tokens_from_account(&account);

    if tokens.access_token.is_empty() || tokens.expires_at == 0 || tokens.expires_at <= now_millis()
    {
        if tokens.refresh_token.is_empty() {
            let message =
                "Codex 登录授权缺少 refresh_token，无法自动续期；当前 access_token 已不可用。";
            mark_account_reauth(paths, account_id, MISSING_REFRESH_TOKEN_REASON, message).await?;
            mark_account_refresh_error(paths, account_id, 0, message).await?;
            return Err(ManagerError::System(message.to_string()));
        }

        tokens = match refresh_token(&tokens.refresh_token, &string_value(account.get("proxy"))).await
        {
            Ok(mut tokens) => {
                tokens.token_generation = number_value(account.get("token_generation"), 0) + 1;
                tokens.token_updated_at = now_millis();
                tokens
            }
            Err(error) => {
                if should_mark_reauth(&error.to_string()) {
                    mark_account_reauth(
                        paths,
                        account_id,
                        "invalid_grant",
                        "Codex 登录授权已失效，请重新登录。",
                    )
                    .await?;
                }
                mark_account_refresh_error(paths, account_id, 0, &error.to_string()).await?;
                return Err(error);
            }
        };
    }

    let mut claims = decode_jwt_payload(&first_string(
        Some(&json!(tokens.id_token.clone())),
        Some(&json!(tokens.access_token.clone())),
        "",
    ))?;
    let account_identity = first_string(
        account.get("account_id"),
        account.get("accountId"),
        &string_value(account.get("id")),
    );
    claims["account_id"] = json!(account_identity);
    let usage = fetch_usage_info(&tokens.access_token, &claims, &string_value(account.get("proxy")))
        .await
        .map_err(|error| {
            ManagerError::System(error.to_string())
        })?;
    let next_account = save_account(
        paths,
        tokens,
        json!({
          "email": first_string(Some(&json!(extract_email(&claims))), account.get("email"), ""),
          "sub": first_string(claims.get("sub"), account.get("accountId"), "")
        }),
        &mut claims,
        usage,
        string_value(account.get("proxy")),
    )
    .await?;

    if string_value(next_account.get("id")) == read_active_account_id(paths)? {
        write_account_bundle(&next_account, cli_target).await?;
    }

    Ok(next_account)
}

async fn prepare_account_for_switch(
    paths: &AppPaths,
    account_id: &str,
    cli_target: &Value,
) -> Result<Value, ManagerError> {
    let mut account = runtime_provider::read_array(&paths.storage_files.codex_accounts)?
        .into_iter()
        .find(|account| string_value(account.get("id")) == account_id)
        .ok_or_else(|| ManagerError::System("Codex 官方账号不存在".to_string()))?;

    if account.get("disabled").and_then(Value::as_bool) == Some(true) {
        return Err(ManagerError::System(
            "Codex 官方账号已禁用，不能启用".to_string(),
        ));
    }

    if account.get("type").and_then(Value::as_str) == Some("apikey") {
        return Ok(account);
    }

    account = sync_account_from_authority_sources(paths, account, cli_target).await?;
    account = clear_missing_refresh_token_reauth(paths, account).await?;

    if account.get("requires_reauth").and_then(Value::as_bool) == Some(true) {
        return Err(ManagerError::System(first_string(
            account.get("reauth_message"),
            account.get("reauth_reason"),
            "Codex 登录授权需要重新登录",
        )));
    }

    if !account_access_token_expired(&account) {
        return Ok(account);
    }

    perform_managed_token_refresh(paths, &account, cli_target).await
}

async fn sync_account_from_authority_sources(
    paths: &AppPaths,
    account: Value,
    cli_target: &Value,
) -> Result<Value, ManagerError> {
    let config_path = string_value(cli_target.get("configPath"));

    if config_path.is_empty() {
        return Ok(account);
    }

    let auth_path = Path::new(&config_path).join("auth.json");
    let auth_data = match tokio::fs::read_to_string(auth_path).await {
        Ok(content) => serde_json::from_str::<Value>(&content)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(account),
        Err(error) => return Err(ManagerError::Io(error)),
    };
    let (mut tokens, mut claims) = create_tokens_from_auth_data(&auth_data)?;
    let source_account_id = extract_account_id(&claims);
    let account_id = first_string(
        account.get("account_id"),
        account.get("accountId"),
        &string_value(account.get("id")),
    );

    if tokens.access_token.is_empty() || source_account_id != account_id {
        return Ok(account);
    }

    let source_updated_at = tokens.token_updated_at.max(parse_timestamp(&string_value(auth_data.get("last_refresh"))));
    let account_updated_at = number_value(account.get("token_updated_at"), 0)
        .max(parse_timestamp(&string_value(account.get("last_refresh"))));
    let should_use_source =
        source_updated_at >= account_updated_at || (account_access_token_expired(&account) && !tokens_access_token_expired(&tokens));

    if !should_use_source {
        return Ok(account);
    }

    tokens.token_generation = number_value(account.get("token_generation"), 0);
    tokens.token_updated_at = if source_updated_at == 0 {
        now_millis()
    } else {
        source_updated_at
    };
    save_account(
        paths,
        tokens,
        json!({
          "email": first_string(Some(&json!(extract_email(&claims))), account.get("email"), ""),
          "sub": first_string(claims.get("sub"), account.get("accountId"), "")
        }),
        &mut claims,
        account.get("usage").cloned().unwrap_or_else(|| json!({})),
        string_value(account.get("proxy")),
    )
    .await
}

async fn clear_missing_refresh_token_reauth(
    paths: &AppPaths,
    account: Value,
) -> Result<Value, ManagerError> {
    if account.get("requires_reauth").and_then(Value::as_bool) != Some(true)
        || string_value(account.get("reauth_reason")) != MISSING_REFRESH_TOKEN_REASON
        || account_refresh_token(&account).is_empty()
    {
        return Ok(account);
    }

    let mut next_account = account;

    next_account["requires_reauth"] = json!(false);
    next_account["reauth_reason"] = json!("");
    next_account["reauth_message"] = json!("");
    next_account["updatedAt"] = json!(now_millis());
    replace_account(paths, &next_account).await?;
    Ok(next_account)
}

async fn perform_managed_token_refresh(
    paths: &AppPaths,
    account: &Value,
    cli_target: &Value,
) -> Result<Value, ManagerError> {
    let refresh = account_refresh_token(account);

    if refresh.is_empty() {
        let message =
            "Codex 登录授权缺少 refresh_token，无法自动续期；当前 access_token 已不可用。";
        mark_account_reauth(
            paths,
            &string_value(account.get("id")),
            MISSING_REFRESH_TOKEN_REASON,
            message,
        )
        .await?;
        return Err(ManagerError::System(message.to_string()));
    }

    let mut tokens = match refresh_token(&refresh, &string_value(account.get("proxy"))).await {
        Ok(tokens) => tokens,
        Err(error) => {
            if should_mark_reauth(&error.to_string()) {
                mark_account_reauth(
                    paths,
                    &string_value(account.get("id")),
                    "invalid_grant",
                    "Codex 登录授权已失效，请重新登录。",
                )
                .await?;
            }
            mark_account_refresh_error(paths, &string_value(account.get("id")), 0, &error.to_string()).await?;
            return Err(error);
        }
    };
    let mut claims = decode_jwt_payload(&first_string(
        Some(&json!(tokens.id_token.clone())),
        Some(&json!(tokens.access_token.clone())),
        "",
    ))?;
    claims["account_id"] = json!(first_string(
        account.get("account_id"),
        account.get("accountId"),
        &string_value(account.get("id")),
    ));
    let usage = fetch_usage_info(&tokens.access_token, &claims, &string_value(account.get("proxy"))).await?;

    tokens.token_generation = number_value(account.get("token_generation"), 0) + 1;
    tokens.token_updated_at = now_millis();
    let next_account = save_account(
        paths,
        tokens,
        json!({
          "email": first_string(Some(&json!(extract_email(&claims))), account.get("email"), ""),
          "sub": first_string(claims.get("sub"), account.get("accountId"), "")
        }),
        &mut claims,
        usage,
        string_value(account.get("proxy")),
    )
    .await?;

    if string_value(next_account.get("id")) == read_active_account_id(paths)? {
        write_account_bundle(&next_account, cli_target).await?;
    }

    Ok(next_account)
}

async fn write_account_bundle(
    account: &Value,
    cli_target: &Value,
) -> Result<(), ManagerError> {
    write_account_auth(account, cli_target).await?;
    write_codex_builtin_config(cli_target).await
}

async fn write_account_auth(account: &Value, cli_target: &Value) -> Result<(), ManagerError> {
    let config_path = string_value(cli_target.get("configPath"));

    if config_path.is_empty() {
        return Err(ManagerError::System("Codex CLI 配置目录不存在".to_string()));
    }

    let access_token = account_access_token(account);

    if access_token.is_empty() {
        return Err(ManagerError::System(
            "OAuth 账号缺少 access_token，无法写入 auth.json".to_string(),
        ));
    }

    tokio::fs::create_dir_all(&config_path).await?;
    tokio::fs::write(
        Path::new(&config_path).join("auth.json"),
        format!(
            "{}\n",
            serde_json::to_string_pretty(&json!({
              "OPENAI_API_KEY": Value::Null,
              "last_refresh": format_rfc3339(now_millis()),
              "tokens": {
                "access_token": access_token,
                "account_id": first_string(account.get("account_id"), account.get("accountId"), &string_value(account.get("id"))),
                "id_token": first_string(account.get("auth").and_then(|auth| auth.get("idToken")), account.get("id_token"), ""),
                "refresh_token": account_refresh_token(account)
              }
            }))?
        ),
    )
    .await?;
    Ok(())
}

async fn write_codex_builtin_config(cli_target: &Value) -> Result<(), ManagerError> {
    let config_path = Path::new(&string_value(cli_target.get("configPath"))).join("config.toml");
    let content = match tokio::fs::read_to_string(&config_path).await {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(ManagerError::Io(error)),
    };
    let without_managed_providers = remove_toml_sections(
        &content,
        &["model_providers.custom", "model_providers.codex_local_access"],
    );
    let next_content = remove_toml_root_keys(
        &without_managed_providers,
        &["model_provider", "openai_base_url"],
    );

    if next_content.is_empty() {
        match tokio::fs::remove_file(config_path).await {
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(ManagerError::Io(error)),
        }
        return Ok(());
    }

    tokio::fs::write(config_path, format!("{}\n", next_content)).await?;
    Ok(())
}

async fn save_account(
    paths: &AppPaths,
    tokens: TokenBundle,
    profile: Value,
    claims: &mut Value,
    usage: Value,
    proxy: String,
) -> Result<Value, ManagerError> {
    let account_id = first_string(
        Some(&json!(extract_account_id(claims))),
        profile
            .get("account_id")
            .or_else(|| profile.get("user_id"))
            .or_else(|| profile.get("sub")),
        &format!("codex-account-{}", uuid::Uuid::new_v4()),
    );
    let email = first_string(
        profile.get("email"),
        Some(&json!(extract_email(claims))),
        "未识别账号",
    );
    let current_account =
        runtime_provider::read_array(&paths.storage_files.codex_accounts)?
            .into_iter()
            .find(|account| string_value(account.get("id")) == account_id);
    let token_updated_at = tokens
        .token_updated_at
        .max(parse_timestamp(&tokens.last_refresh))
        .max(now_millis());
    let next_account = json!({
      "id": account_id,
      "provider": "codex",
      "type": "codex",
      "accountId": account_id,
      "account_id": account_id,
      "email": email,
      "plan": first_string(
        usage.get("plan_type"),
        profile.get("chatgpt_plan_type").or_else(|| claims.get("chatgpt_plan_type")),
        ""
      ),
      "usage": usage,
      "proxy": if proxy.is_empty() {
        current_account.as_ref().map(|account| string_value(account.get("proxy"))).unwrap_or_default()
      } else {
        proxy
      },
      "id_token": tokens.id_token,
      "access_token": tokens.access_token,
      "refresh_token": tokens.refresh_token,
      "last_refresh": tokens.last_refresh,
      "expired": tokens.expired,
      "auth": {
        "accessToken": tokens.access_token,
        "refreshToken": tokens.refresh_token,
        "idToken": tokens.id_token,
        "expiresAt": tokens.expires_at,
        "access_token": tokens.access_token,
        "refresh_token": tokens.refresh_token,
        "id_token": tokens.id_token,
        "last_refresh": tokens.last_refresh,
        "expired": tokens.expired,
        "token_updated_at": token_updated_at
      },
      "token_generation": tokens.token_generation.max(current_account.as_ref().map(|account| number_value(account.get("token_generation"), 0)).unwrap_or(0)),
      "token_updated_at": token_updated_at,
      "refresh_status": "",
      "refresh_status_code": 0,
      "refresh_message": "",
      "requires_reauth": false,
      "reauth_reason": "",
      "reauth_message": "",
      "autoRefresh": current_account.as_ref().and_then(|account| account.get("autoRefresh")).and_then(Value::as_bool).unwrap_or(true),
      "disabled": current_account.as_ref().and_then(|account| account.get("disabled")).and_then(Value::as_bool).unwrap_or(false),
      "createdAt": current_account.as_ref().map(|account| number_value(account.get("createdAt"), now_millis())).unwrap_or_else(now_millis),
      "updatedAt": now_millis()
    });

    replace_account(paths, &next_account).await?;
    Ok(next_account)
}

async fn replace_account(paths: &AppPaths, next_account: &Value) -> Result<(), ManagerError> {
    let account_id = string_value(next_account.get("id"));
    let mut replaced = false;
    let mut accounts = runtime_provider::read_array(&paths.storage_files.codex_accounts)?
        .into_iter()
        .map(|account| {
            if string_value(account.get("id")) == account_id {
                replaced = true;
                next_account.clone()
            } else {
                account
            }
        })
        .collect::<Vec<_>>();

    if !replaced {
        accounts.push(next_account.clone());
    }

    runtime_provider::write_json(&paths.storage_files.codex_accounts, &json!(accounts)).await
}

async fn mark_account_reauth(
    paths: &AppPaths,
    account_id: &str,
    reason: &str,
    message: &str,
) -> Result<(), ManagerError> {
    update_account(paths, account_id, |account| {
        account["requires_reauth"] = json!(true);
        account["reauth_reason"] = json!(reason);
        account["reauth_message"] = json!(message);
        account["updatedAt"] = json!(now_millis());
    })
    .await
}

async fn mark_account_refresh_error(
    paths: &AppPaths,
    account_id: &str,
    status_code: u16,
    message: &str,
) -> Result<(), ManagerError> {
    update_account(paths, account_id, |account| {
        account["refresh_status"] = json!("failed");
        account["refresh_status_code"] = json!(status_code);
        account["refresh_message"] = json!(message);
        account["updatedAt"] = json!(now_millis());
    })
    .await
}

async fn update_account(
    paths: &AppPaths,
    account_id: &str,
    updater: impl Fn(&mut Value),
) -> Result<(), ManagerError> {
    let accounts = runtime_provider::read_array(&paths.storage_files.codex_accounts)?
        .into_iter()
        .map(|mut account| {
            if string_value(account.get("id")) == account_id {
                updater(&mut account);
            }
            account
        })
        .collect::<Vec<_>>();

    runtime_provider::write_json(&paths.storage_files.codex_accounts, &json!(accounts)).await
}

async fn fetch_usage_info(
    access_token: &str,
    claims: &Value,
    proxy: &str,
) -> Result<Value, ManagerError> {
    let account_id = extract_account_id(claims);
    let mut headers = HeaderMap::new();

    headers.insert("content-type", HeaderValue::from_static("application/json"));
    headers.insert("cache-control", HeaderValue::from_static("no-cache"));
    headers.insert(
        "authorization",
        HeaderValue::from_str(&format!("Bearer {}", access_token))
            .map_err(|error| ManagerError::System(error.to_string()))?,
    );
    headers.insert(
        "user-agent",
        HeaderValue::from_static(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36",
        ),
    );
    if !account_id.is_empty() {
        headers.insert(
            "chatgpt-account-id",
            HeaderValue::from_str(&account_id)
                .map_err(|error| ManagerError::System(error.to_string()))?,
        );
    }

    let response = http_client(proxy)?
        .get(CODEX_USAGE_URL)
        .headers(headers)
        .send()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;

    read_response_json(response).await
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

async fn read_response_json(response: reqwest::Response) -> Result<Value, ManagerError> {
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| ManagerError::System(error.to_string()))?;

    if !status.is_success() {
        return Err(ManagerError::System(format!(
            "OpenAI 请求失败：{} {}",
            status.as_u16(),
            text
        )));
    }

    Ok(serde_json::from_str(&text)?)
}

fn validate_import_target(
    accounts: &[Value],
    target_account: Option<&Value>,
    target_account_id: &str,
    account_id: &str,
) -> Result<(), ManagerError> {
    if !target_account_id.is_empty() && target_account.is_none() {
        return Err(ManagerError::System("Codex 官方账号不存在".to_string()));
    }

    if target_account
        .and_then(|account| account.get("disabled"))
        .and_then(Value::as_bool)
        == Some(true)
    {
        return Err(ManagerError::System(
            "Codex 官方账号已禁用，不能编辑".to_string(),
        ));
    }

    if let Some(target_account) = target_account {
        let allowed = [
            string_value(target_account.get("id")),
            string_value(target_account.get("accountId")),
            string_value(target_account.get("account_id")),
        ];

        if !allowed.iter().any(|item| item == account_id) {
            return Err(ManagerError::System("登录数据与当前账号不一致".to_string()));
        }
    }

    if !account_id.is_empty()
        && accounts.iter().any(|account| {
            string_value(account.get("id")) != target_account_id
                && [
                    string_value(account.get("id")),
                    string_value(account.get("accountId")),
                    string_value(account.get("account_id")),
                ]
                .iter()
                .any(|item| item == account_id)
        })
    {
        return Err(ManagerError::System("此账户已导入".to_string()));
    }

    Ok(())
}

fn public_account(account: &Value, active_account_id: &str, include_auth: bool) -> Value {
    let mut output = json!({
      "id": account["id"],
      "provider": account["provider"],
      "accountId": account["accountId"],
      "account_id": account["account_id"],
      "email": account["email"],
      "plan": account["plan"],
      "usage": account["usage"],
      "proxy": account["proxy"],
      "autoRefresh": account["autoRefresh"],
      "createdAt": account["createdAt"],
      "updatedAt": account["updatedAt"],
      "last_refresh": account["last_refresh"],
      "expired": account["expired"],
      "type": account["type"],
      "token_generation": number_value(account.get("token_generation"), 0),
      "token_updated_at": number_value(account.get("token_updated_at"), 0),
      "refresh_status": string_value(account.get("refresh_status")),
      "refresh_status_code": number_value(account.get("refresh_status_code"), 0),
      "refresh_message": string_value(account.get("refresh_message")),
      "requires_reauth": account.get("requires_reauth").and_then(Value::as_bool).unwrap_or(false),
      "reauth_reason": string_value(account.get("reauth_reason")),
      "reauth_message": string_value(account.get("reauth_message")),
      "disabled": account.get("disabled").and_then(Value::as_bool).unwrap_or(false),
      "active": string_value(account.get("id")) == active_account_id,
      "auth": {
        "expiresAt": account_expires_at(account)
      }
    });

    if include_auth {
        output["auth"] = account.get("auth").cloned().unwrap_or_else(|| json!({}));
    }

    output
}

fn public_login_state(state: Option<Value>) -> Value {
    let Some(state) = state else {
        return Value::Null;
    };

    json!({
      "status": state["status"],
      "authUrl": state["authUrl"],
      "redirectUri": state["redirectUri"],
      "message": state.get("message").cloned().unwrap_or_else(|| json!("")),
      "account": state.get("account").cloned().unwrap_or(Value::Null)
    })
}

fn create_tokens_from_auth_data(auth_data: &Value) -> Result<(TokenBundle, Value), ManagerError> {
    let token_source = auth_data.get("tokens").unwrap_or(auth_data);
    let access_token = first_string(
        token_source.get("access_token"),
        token_source.get("accessToken"),
        "",
    );
    let id_token = first_string(
        token_source.get("id_token"),
        token_source.get("idToken").or_else(|| token_source.get("id_otkne")),
        "",
    );
    let mut claims = decode_jwt_payload(&first_string(
        Some(&json!(id_token.clone())),
        Some(&json!(access_token.clone())),
        "",
    ))?;
    let expires_at = number_value(token_source.get("expiresAt"), number_value(auth_data.get("expiresAt"), 0))
        .max(parse_timestamp(&first_string(token_source.get("expired"), auth_data.get("expired"), "")))
        .max(number_value(claims.get("exp"), 0) * 1000);
    let account_id = first_string(token_source.get("account_id"), None, "");

    if !account_id.is_empty() {
        claims["sub"] = json!(account_id);
        claims["account_id"] = json!(account_id);
    }

    Ok((
        TokenBundle {
            access_token,
            refresh_token: first_string(
                token_source.get("refresh_token"),
                token_source.get("refreshToken"),
                "",
            ),
            id_token,
            expires_at,
            last_refresh: first_string(
                token_source.get("last_refresh"),
                auth_data.get("last_refresh"),
                "",
            ),
            expired: first_string(token_source.get("expired"), auth_data.get("expired"), "")
                .if_empty_then(|| if expires_at == 0 { String::new() } else { format_rfc3339(expires_at) }),
            token_generation: number_value(token_source.get("token_generation"), 0),
            token_updated_at: number_value(
                token_source.get("token_updated_at"),
                number_value(auth_data.get("token_updated_at"), 0),
            )
            .max(parse_timestamp(&first_string(
                token_source.get("last_refresh"),
                auth_data.get("last_refresh"),
                "",
            ))),
        },
        claims,
    ))
}

fn tokens_from_account(account: &Value) -> TokenBundle {
    TokenBundle {
        access_token: first_string(
            account.get("auth").and_then(|auth| auth.get("accessToken")),
            account.get("access_token"),
            "",
        ),
        refresh_token: first_string(
            account.get("auth").and_then(|auth| auth.get("refreshToken")),
            account.get("refresh_token"),
            "",
        ),
        id_token: first_string(
            account.get("auth").and_then(|auth| auth.get("idToken")),
            account.get("id_token"),
            "",
        ),
        expires_at: account_expires_at(account),
        last_refresh: string_value(account.get("last_refresh")),
        expired: string_value(account.get("expired")),
        token_generation: number_value(account.get("token_generation"), 0),
        token_updated_at: number_value(account.get("token_updated_at"), 0)
            .max(parse_timestamp(&string_value(account.get("last_refresh")))),
    }
}

fn decode_jwt_payload(token: &str) -> Result<Value, ManagerError> {
    let Some(payload) = token.split('.').nth(1) else {
        return Ok(json!({}));
    };
    let mut normalized = payload.replace('-', "+").replace('_', "/");

    while normalized.len() % 4 != 0 {
        normalized.push('=');
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(normalized)
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok(serde_json::from_slice(&bytes).unwrap_or_else(|_| json!({})))
}

fn extract_account_id(claims: &Value) -> String {
    let auth_claims = claims
        .get("https://api.openai.com/auth")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    first_string(
        auth_claims.get("chatgpt_account_id"),
        claims.get("account_id").or_else(|| claims.get("sub")),
        "",
    )
}

fn extract_email(claims: &Value) -> String {
    let profile_claims = claims
        .get("https://api.openai.com/profile")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    first_string(profile_claims.get("email"), claims.get("email"), "")
}

fn account_expires_at(account: &Value) -> u64 {
    number_value(
        account.get("auth").and_then(|auth| auth.get("expiresAt")),
        parse_timestamp(&string_value(account.get("expired"))),
    )
}

fn account_access_token(account: &Value) -> String {
    first_string(
        account.get("auth").and_then(|auth| auth.get("accessToken")),
        account.get("access_token"),
        "",
    )
}

fn account_refresh_token(account: &Value) -> String {
    first_string(
        account.get("auth").and_then(|auth| auth.get("refreshToken")),
        account.get("refresh_token"),
        "",
    )
}

fn account_access_token_expired(account: &Value) -> bool {
    let expires_at = account_expires_at(account);

    account_access_token(account).is_empty() || expires_at == 0 || expires_at <= now_millis()
}

fn tokens_access_token_expired(tokens: &TokenBundle) -> bool {
    tokens.access_token.is_empty() || tokens.expires_at == 0 || tokens.expires_at <= now_millis()
}

fn should_mark_reauth(message: &str) -> bool {
    [
        "refresh_token_reused",
        "refresh_token_expired",
        "refresh_token_invalidated",
        "invalid_grant",
    ]
    .iter()
    .any(|item| message.contains(item))
}

fn remove_toml_sections(content: &str, section_names: &[&str]) -> String {
    let mut next_lines = Vec::new();
    let mut skipping = false;

    for line in content.lines() {
        let text = line.trim();

        if text.starts_with('[') && text.ends_with(']') {
            let section = text.trim_start_matches('[').trim_end_matches(']');
            skipping = section_names.contains(&section);

            if skipping {
                continue;
            }
        }

        if !skipping {
            next_lines.push(line.to_string());
        }
    }

    next_lines.join("\n")
}

fn remove_toml_root_keys(content: &str, keys: &[&str]) -> String {
    let mut next_lines = Vec::new();
    let mut in_section = false;

    for line in content.lines() {
        if line.trim().starts_with('[') {
            in_section = true;
        }

        let key = line
            .trim()
            .split_once('=')
            .map(|(key, _)| key.trim().to_string())
            .unwrap_or_default();

        if !in_section && keys.contains(&key.as_str()) {
            continue;
        }

        next_lines.push(line.to_string());
    }

    regex::Regex::new(r"\n{3,}")
        .map(|regex| regex.replace_all(&next_lines.join("\n"), "\n\n").trim().to_string())
        .unwrap_or_else(|_| next_lines.join("\n").trim().to_string())
}

fn find_codex_cli_target(cli_targets: &Value) -> Result<Value, ManagerError> {
    runtime_provider::find_cli_target(cli_targets, "codex")
}

fn read_active_account_id(paths: &AppPaths) -> Result<String, ManagerError> {
    match std::fs::read_to_string(&paths.storage_files.codex_active_account_id) {
        Ok(content) => Ok(serde_json::from_str::<Value>(&content)
            .map(|value| string_value(Some(&value)))
            .unwrap_or_default()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(ManagerError::Io(error)),
    }
}

fn build_auth_url(challenge: &str, state: &str) -> Result<String, ManagerError> {
    let mut url = url::Url::parse(&format!("{}/oauth/authorize", OAUTH_BASE_URL))
        .map_err(|error| ManagerError::System(error.to_string()))?;

    url.query_pairs_mut()
        .append_pair("client_id", CODEX_CLIENT_ID)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("id_token_add_organizations", "true")
        .append_pair("prompt", "login")
        .append_pair("redirect_uri", OAUTH_REDIRECT_URI)
        .append_pair("response_type", "code")
        .append_pair("scope", OAUTH_SCOPE)
        .append_pair("state", state);

    Ok(url.to_string())
}

fn text_response(status: StatusCode, text: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-type", "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from(text.to_string())))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::from(text.to_string()))))
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

fn random_url_token(length: usize) -> Result<String, ManagerError> {
    let mut bytes = vec![0u8; length];

    getrandom::getrandom(&mut bytes).map_err(|error| ManagerError::System(error.to_string()))?;
    Ok(base64_url_no_pad(&bytes))
}

fn random_hex(length: usize) -> Result<String, ManagerError> {
    let mut bytes = vec![0u8; length];

    getrandom::getrandom(&mut bytes).map_err(|error| ManagerError::System(error.to_string()))?;
    Ok(bytes.iter().map(|byte| format!("{:02x}", byte)).collect())
}

fn base64_url_no_pad(value: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(value)
}

fn parse_timestamp(value: &str) -> u64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|time| time.timestamp_millis().max(0) as u64)
        .unwrap_or(0)
}

fn format_rfc3339(timestamp: u64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(timestamp as i64)
        .map(|time| time.to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
        .unwrap_or_default()
}

fn now_millis() -> u64 {
    runtime_provider::now_millis()
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

trait EmptyStringExt {
    fn if_empty_then(self, value: impl FnOnce() -> String) -> String;
}

impl EmptyStringExt for String {
    fn if_empty_then(self, value: impl FnOnce() -> String) -> String {
        if self.is_empty() {
            value()
        } else {
            self
        }
    }
}

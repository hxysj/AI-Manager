use crate::api::{google_gateway, proxy, runtime_provider};
use crate::core::{database, error::ManagerError, paths::AppPaths, provider_store};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use bytes::Bytes;
use futures_util::FutureExt;
use http_body_util::Full;
use hyper::{body::Incoming, server::conn::http1, service::service_fn, Request, Response};
use hyper_util::rt::TokioIo;
use rusqlite::params;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    convert::Infallible,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tauri_plugin_opener::OpenerExt;
use tokio::{
    net::TcpListener,
    sync::{oneshot, Mutex},
    task::{JoinHandle, JoinSet},
};

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const ENDPOINTS: [&str; 3] = [
    "https://daily-cloudcode-pa.sandbox.googleapis.com/v1internal",
    "https://daily-cloudcode-pa.googleapis.com/v1internal",
    "https://cloudcode-pa.googleapis.com/v1internal",
];
static TOKEN_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Default)]
pub struct GoogleLogin {
    state: Arc<Mutex<Value>>,
    task: Option<JoinHandle<()>>,
}

impl Drop for GoogleLogin {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl GoogleLogin {
    pub async fn state(&self) -> Value {
        self.state.lock().await.clone()
    }

    pub async fn cancel(&mut self) -> Value {
        if let Some(task) = self.task.take() {
            task.abort();
        }
        *self.state.lock().await = json!({"status": "cancelled", "message": "已取消 Google 登录"});
        self.state().await
    }

    pub async fn start(
        &mut self,
        app: &tauri::AppHandle,
        paths: &AppPaths,
        payload: Value,
    ) -> Result<Value, ManagerError> {
        if self.task.as_ref().is_some_and(|task| !task.is_finished()) {
            return Err(error("Google 登录正在进行中"));
        }
        let proxy_url = payload["proxy"].as_str().unwrap_or("").trim().to_string();
        proxy::http_client(&proxy_url)?;
        let target = payload["providerId"].as_str().unwrap_or("").to_string();
        if !target.is_empty() {
            find_account(paths, &target)?;
        }
        // 使用明确的 IPv4 回环地址，避免 localhost 的 IPv4/IPv6 解析差异。
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let redirect = format!("http://{}/oauth-callback", listener.local_addr()?);
        let mut random = [0u8; 32];
        getrandom::getrandom(&mut random).map_err(|_| error("无法生成 OAuth 随机凭据"))?;
        let verifier = URL_SAFE_NO_PAD.encode(random);
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let state = uuid::Uuid::new_v4().to_string();
        let client_id = std::env::var("GOOGLE_OAUTH_CLIENT_ID").unwrap_or_else(|_| {
            "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com".into()
        });
        let client_secret = std::env::var("GOOGLE_OAUTH_CLIENT_SECRET")
            .unwrap_or_else(|_| "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".into());
        let mut url = url::Url::parse("https://accounts.google.com/o/oauth2/v2/auth").unwrap();
        url.query_pairs_mut().extend_pairs([
            ("client_id", client_id.as_str()), ("redirect_uri", redirect.as_str()), ("response_type", "code"),
            ("scope", "openid https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cclog https://www.googleapis.com/auth/experimentsandconfigs"),
            ("access_type", "offline"), ("prompt", "consent"), ("include_granted_scopes", "true"),
            ("state", state.as_str()), ("code_challenge", challenge.as_str()), ("code_challenge_method", "S256"),
        ]);
        let auth_url = url.to_string();
        *self.state.lock().await = json!({"status": "pending", "authUrl": auth_url, "message": "请在浏览器完成 Google 授权"});
        let login_state = self.state.clone();
        let paths = paths.clone();
        self.task = Some(tokio::spawn(async move {
            let result = async {
                let code = receive_code(listener, state).await?;
                let client = proxy::http_client(&proxy_url)?;
                let response = client.post(TOKEN_URL).form(&[
                    ("client_id", client_id.as_str()), ("client_secret", client_secret.as_str()),
                    ("redirect_uri", redirect.as_str()), ("code", code.as_str()), ("code_verifier", verifier.as_str()),
                    ("grant_type", "authorization_code"),
                ]).send().await.map_err(|_| error("Google Token 接口连接失败，请检查网络代理"))?;
                let mut tokens = token_json(response).await?;
                let access = tokens["access_token"].as_str().ok_or_else(|| error("Google 未返回 access_token"))?;
                let response = client.get("https://www.googleapis.com/oauth2/v2/userinfo").bearer_auth(access).send().await
                    .map_err(|_| error("Google 账号信息接口连接失败"))?;
                let info = json_response(response).await?;
                let subject = info["id"].as_str().filter(|value| !value.is_empty()).ok_or_else(|| error("Google 未返回账号 ID"))?;
                let id = format!("google-{}", subject);
                if !target.is_empty() && target != id { return Err(error("登录账号与待更新的 Google 账号不一致")); }
                let old = provider_store::read_providers(&paths)?.into_iter().find(|item| item["id"] == id);
                if tokens["refresh_token"].as_str().unwrap_or("").is_empty() {
                    if let Ok(previous) = read_tokens(&paths, &id) {
                        // refresh_token 只能在同一 OAuth client 下复用。
                        if previous["client_id"] == client_id { tokens["refresh_token"] = previous["refresh_token"].clone(); }
                    }
                    if tokens["refresh_token"].as_str().unwrap_or("").is_empty() {
                        return Err(error("Google 未返回 refresh_token，请撤销该应用授权后重新登录"));
                    }
                }
                tokens["client_id"] = json!(client_id);
                tokens["client_secret"] = json!(client_secret);
                tokens["expires_at"] = json!(chrono::Utc::now().timestamp() + tokens["expires_in"].as_i64().unwrap_or(3600));
                let port = if let Some(port) = old.as_ref().and_then(|item| item["google"]["port"].as_u64()) { port as u16 } else {
                    std::net::TcpListener::bind("127.0.0.1:0")?.local_addr()?.port()
                };
                let mut provider = old.unwrap_or_else(|| json!({"id": id, "cli": "codex", "type": "google-account", "icon": "google", "enabled": true,
                    "runtimeConfig": {}, "headers": {}, "proxy": "", "createdAt": chrono::Utc::now().timestamp_millis()}));
                provider["name"] = json!(info["email"].as_str().unwrap_or("Google 账号"));
                provider["baseUrl"] = json!(format!("http://127.0.0.1:{port}/v1"));
                provider["google"] = json!({"subject": subject, "email": info["email"], "name": info["name"], "picture": info["picture"],
                    "proxy": proxy_url, "port": port, "status": "ready", "models": [], "message": ""});
                // 先持久化授权，再获取额度；额度接口失败不丢失刚获得的 refresh_token。
                save_login(&paths, &provider, &tokens)?;
                let warning = refresh(&paths, &id).await.err().map(|error| error.to_string());
                Ok::<_, ManagerError>(json!({"status": "success", "providerId": id,
                    "message": warning.unwrap_or_else(|| "Google 登录完成，已获取模型与额度".into())}))
            }.await;
            *login_state.lock().await = match result {
                Ok(state) => state,
                Err(error) => json!({"status": "failed", "message": error.to_string()}),
            };
        }));
        // 浏览器启动失败时保留回调监听和授权链接，用户可复制链接完成登录。
        if app.opener().open_url(auth_url, None::<&str>).is_err() {
            self.state.lock().await["message"] = json!("浏览器未能自动打开，请复制授权链接后打开");
        }
        Ok(self.state().await)
    }
}

async fn receive_code(
    listener: TcpListener,
    expected_state: String,
) -> Result<String, ManagerError> {
    let (sender, receiver) = oneshot::channel::<Result<String, String>>();
    let sender = Arc::new(Mutex::new(Some(sender)));
    let mut connections = JoinSet::new();
    let expected_state = Arc::new(expected_state);
    let deadline = tokio::time::sleep(Duration::from_secs(600)).fuse();
    let receiver = receiver.fuse();
    tokio::pin!(deadline);
    tokio::pin!(receiver);
    loop {
        futures_util::select! {
            result = &mut receiver => {
                // 等待已接收回调的 HTTP 响应写回浏览器。
                let _ = tokio::time::timeout(Duration::from_secs(2), async { while connections.join_next().await.is_some() {} }).await;
                return result.map_err(|_| error("Google 登录已中断"))?.map_err(|message| error(&message));
            },
            _ = &mut deadline => return Err(error("Google 授权已过期，请重新登录")),
            accepted = listener.accept().fuse() => {
                let (stream, _) = accepted?;
                let sender = sender.clone();
                let state = expected_state.clone();
                connections.spawn(async move {
                    let _ = http1::Builder::new().keep_alive(false).serve_connection(TokioIo::new(stream), service_fn(move |request: Request<Incoming>| {
                        let sender = sender.clone();
                        let state = state.clone();
                        async move {
                            let url = url::Url::parse(&format!("http://127.0.0.1{}", request.uri())).unwrap();
                            let query: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
                            let valid = request.method() == hyper::Method::GET && url.path() == "/oauth-callback" && query.get("state") == Some(state.as_ref());
                            let (status, message) = if valid {
                                let result = if query.contains_key("error") { Err("Google 授权被拒绝，请重新登录".into()) }
                                    else { query.get("code").filter(|code| !code.is_empty()).cloned().ok_or_else(|| "Google 回调缺少授权码".into()) };
                                if let Some(sender) = sender.lock().await.take() { let _ = sender.send(result); }
                                (200, "已收到 Google 授权结果，请返回 Monkey Thief 查看登录状态。")
                            } else { (400, "无效的授权回调，请从应用重新发起 Google 登录。") };
                            Ok::<_, Infallible>(Response::builder().status(status).header("content-type", "text/plain; charset=utf-8")
                                .body(Full::new(Bytes::from(message))).unwrap())
                        }
                    })).await;
                });
                while connections.try_join_next().is_some() {}
            }
        }
    }
}

pub(crate) fn find_account(paths: &AppPaths, id: &str) -> Result<Value, ManagerError> {
    let provider = runtime_provider::find_provider(paths, id)?;
    if provider["type"] != "google-account" {
        return Err(error("不是 Google 账号服务商"));
    }
    Ok(provider)
}

fn save_login(paths: &AppPaths, provider: &Value, tokens: &Value) -> Result<(), ManagerError> {
    provider_store::initialize(paths)?;
    let id = provider["id"].as_str().unwrap();
    let mut connection = database::open(paths)?;
    let transaction = connection.transaction()?;
    transaction.execute("INSERT INTO providers(item_key, sort_order, payload_json) VALUES (?1, (SELECT COALESCE(MAX(sort_order), 0) + 1 FROM providers), ?2) ON CONFLICT(item_key) DO UPDATE SET payload_json=excluded.payload_json", params![id, provider.to_string()])?;
    transaction.execute("INSERT INTO provider_keys(provider_id, payload_json) VALUES (?1, ?2) ON CONFLICT(provider_id) DO UPDATE SET payload_json=excluded.payload_json",
        params![format!("google-oauth:{id}"), json!(runtime_provider::encrypt_provider_key(&tokens.to_string())?).to_string()])?;
    let token = json!(runtime_provider::encrypt_provider_key(
        &uuid::Uuid::new_v4().to_string()
    )?)
    .to_string();
    transaction.execute(
        "INSERT OR IGNORE INTO provider_keys(provider_id, payload_json) VALUES (?1, ?2)",
        params![id, token],
    )?;
    transaction.commit()?;
    Ok(())
}

fn read_tokens(paths: &AppPaths, id: &str) -> Result<Value, ManagerError> {
    let value = runtime_provider::get_provider_api_key(paths, &format!("google-oauth:{id}"))?;
    if value.is_empty() {
        return Err(error("Google 授权信息不存在，请重新登录"));
    }
    Ok(serde_json::from_str(&value)?)
}

fn save_tokens(paths: &AppPaths, id: &str, tokens: &Value) -> Result<(), ManagerError> {
    let connection = database::open(paths)?;
    let count = connection.execute(
        "UPDATE provider_keys SET payload_json=?2 WHERE provider_id=?1",
        params![
            format!("google-oauth:{id}"),
            json!(runtime_provider::encrypt_provider_key(&tokens.to_string())?).to_string()
        ],
    )?;
    if count == 0 {
        return Err(error("Google 账号已删除"));
    }
    Ok(())
}

pub(crate) async fn access_token(
    paths: &AppPaths,
    id: &str,
    force: bool,
) -> Result<String, ManagerError> {
    let _guard = TOKEN_LOCK.get_or_init(|| Mutex::new(())).lock().await;
    let provider = find_account(paths, id)?;
    if provider["enabled"] == false {
        return Err(error("Google 账号已禁用"));
    }
    let mut tokens = read_tokens(paths, id)?;
    if force || tokens["expires_at"].as_i64().unwrap_or(0) <= chrono::Utc::now().timestamp() + 900 {
        let client = proxy::http_client(provider["google"]["proxy"].as_str().unwrap_or(""))?;
        let response = client
            .post(TOKEN_URL)
            .form(&[
                ("client_id", tokens["client_id"].as_str().unwrap_or("")),
                (
                    "client_secret",
                    tokens["client_secret"].as_str().unwrap_or(""),
                ),
                (
                    "refresh_token",
                    tokens["refresh_token"].as_str().unwrap_or(""),
                ),
                ("grant_type", "refresh_token"),
            ])
            .send()
            .await
            .map_err(|_| error("Google Token 刷新连接失败"))?;
        let refreshed = match token_json(response).await {
            Ok(value) => value,
            Err(failure) => {
                let message = failure.to_string();
                if message.contains("invalid_grant") {
                    update_metadata(
                        paths,
                        id,
                        json!({"status": "reauth_required", "message": "Google 授权已失效，请重新登录"}),
                    )?;
                }
                return Err(failure);
            }
        };
        tokens["access_token"] = refreshed["access_token"].clone();
        tokens["expires_at"] = json!(
            chrono::Utc::now().timestamp() + refreshed["expires_in"].as_i64().unwrap_or(3600)
        );
        if refreshed["refresh_token"].is_string() {
            tokens["refresh_token"] = refreshed["refresh_token"].clone();
        }
        save_tokens(paths, id, &tokens)?;
    }
    tokens["access_token"]
        .as_str()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| error("Google 访问凭据无效，请重新登录"))
}

pub(crate) async fn upstream(
    paths: &AppPaths,
    id: &str,
    method: &str,
    body: &Value,
) -> Result<reqwest::Response, ManagerError> {
    let provider = find_account(paths, id)?;
    let client = proxy::http_client(provider["google"]["proxy"].as_str().unwrap_or(""))?;
    let mut token = access_token(paths, id, false).await?;
    let mut last_error = "Google 服务连接失败，请检查网络代理".to_string();
    for endpoint in ENDPOINTS {
        for attempt in 0..2 {
            let result = client
                .post(format!("{endpoint}:{method}"))
                .bearer_auth(&token)
                .header("user-agent", "antigravity/4.7.11")
                .header("x-client-name", "antigravity")
                .header("x-client-version", "4.7.11")
                .header(
                    "x-machine-id",
                    provider["id"].as_str().unwrap_or("ai-manager"),
                )
                .header("x-vscode-sessionid", uuid::Uuid::new_v4().to_string())
                .json(body)
                .send()
                .await;
            let response = match result {
                Ok(response) => response,
                Err(_) => {
                    break;
                }
            };
            let status = response.status();
            if status.as_u16() == 401 && attempt == 0 {
                token = access_token(paths, id, true).await?;
                continue;
            }
            if matches!(status.as_u16(), 404 | 408) || status.is_server_error() {
                last_error = format!("Google 端点返回 HTTP {}", status.as_u16());
                break;
            }
            return Ok(response);
        }
    }
    Err(error(&last_error))
}

pub async fn refresh(paths: &AppPaths, id: &str) -> Result<Value, ManagerError> {
    let result = async {
        let assist = json_response(upstream(paths, id, "loadCodeAssist", &json!({"metadata": {"ideType": "ANTIGRAVITY"}})).await?).await?;
        let project = assist["cloudaicompanionProject"].as_str().or_else(|| assist["cloudaicompanionProject"]["id"].as_str()).unwrap_or("");
        let plan = assist["paidTier"]["id"].as_str().or_else(|| assist["currentTier"]["id"].as_str()).unwrap_or("FREE");
        let available = json_response(upstream(paths, id, "fetchAvailableModels", &json!({"project": project})).await?).await?;
        let (summary, warning) = match upstream(paths, id, "retrieveUserQuotaSummary", &json!({"project": project})).await {
            Ok(response) => match json_response(response).await { Ok(summary) => (summary, String::new()), Err(failure) => (Value::Null, failure.to_string()) },
            Err(failure) => (Value::Null, failure.to_string()),
        };
        let models = parse_models(&available, &summary)?;
        update_metadata(paths, id, json!({"project": project, "plan": plan, "models": models,
            "quotaGroups": summary["groups"], "quotaWarning": warning, "deprecatedModelIds": available["deprecatedModelIds"],
            "updatedAt": chrono::Utc::now().timestamp_millis(), "status": if project.is_empty() { "setup_required" } else { "ready" },
            "message": if project.is_empty() { "Google 未返回 Cloud Code 项目，请先在对应 Google 服务中完成开通，再刷新" } else { "" }}))?;
        let mut provider = find_account(paths, id)?;
        if provider["runtimeConfig"]["mainModel"].as_str().unwrap_or("").is_empty() {
            provider["runtimeConfig"]["mainModel"] = json!(models.iter().find(|model| model["recommended"] == true).or_else(|| models.first()).map(|model| model["id"].as_str().unwrap()).unwrap_or(""));
            write_provider(paths, &provider)?;
        }
        // 模型同步只改当前账号的行，避免覆盖其他服务商并发保存的结果。
        let mut connection = database::open(paths)?;
        let transaction = connection.transaction()?;
        transaction.execute("DELETE FROM provider_models WHERE json_extract(payload_json, '$.providerId')=?1", [id])?;
        for (index, model) in models.iter().enumerate() {
            let key = format!("{id}:{}", model["id"].as_str().unwrap());
            let record = json!({"id": key, "providerId": id, "name": model["id"], "contextWindow": model["maxTokens"], "enabled": true});
            transaction.execute("INSERT INTO provider_models(item_key, sort_order, payload_json) VALUES (?1, ?2, ?3)", params![key, index, record.to_string()])?;
        }
        transaction.commit()?;
        Ok::<_, ManagerError>(provider)
    }.await;
    if let Err(failure) = &result {
        let account = find_account(paths, id)?;
        let status = if account["google"]["status"] == "reauth_required" {
            "reauth_required"
        } else {
            "error"
        };
        update_metadata(
            paths,
            id,
            json!({"status": status, "message": failure.to_string()}),
        )?;
    }
    result
}

pub(crate) fn parse_models(available: &Value, summary: &Value) -> Result<Vec<Value>, ManagerError> {
    let models = available["models"]
        .as_object()
        .ok_or_else(|| error("Google 未返回有效模型列表"))?;
    let mut result = Vec::new();
    for (id, info) in models {
        if !info.is_object() || available["deprecatedModelIds"].get(id).is_some() {
            continue;
        }
        let mut fraction = info["quotaInfo"]["remainingFraction"].as_f64();
        let mut reset = info["quotaInfo"]["resetTime"].clone();
        let mut buckets = Vec::new();
        for group in summary["groups"].as_array().into_iter().flatten() {
            for bucket in group["buckets"].as_array().into_iter().flatten() {
                let bucket_id = bucket["bucketId"].as_str().unwrap_or("").replace('_', "-");
                let explicit = bucket["modelIds"]
                    .as_array()
                    .or_else(|| group["modelIds"].as_array());
                let matched = explicit
                    .map(|ids| ids.iter().any(|item| item == id))
                    .unwrap_or_else(|| {
                        !bucket_id.is_empty()
                            && (id == &bucket_id || id.starts_with(&format!("{bucket_id}-")))
                    });
                if !matched {
                    continue;
                }
                buckets.push(bucket.clone());
                if let Some(value) = bucket["remainingFraction"].as_f64() {
                    if fraction.is_none_or(|previous| value < previous) {
                        fraction = Some(value);
                        reset = bucket["resetTime"].clone();
                    }
                }
            }
        }
        result.push(json!({"id": id, "displayName": info["displayName"].as_str().unwrap_or(id),
            "remainingPercent": fraction.map(|value| (value.clamp(0.0, 1.0) * 1000.0).round() / 10.0),
            "resetTime": reset, "quotaBuckets": buckets, "supportsThinking": info["supportsThinking"],
            "maxTokens": info["maxTokens"], "maxOutputTokens": info["maxOutputTokens"], "recommended": info["recommended"]}));
    }
    result.sort_by(|left, right| left["id"].as_str().cmp(&right["id"].as_str()));
    Ok(result)
}

pub async fn save_settings(
    paths: &AppPaths,
    cli_targets: &Value,
    payload: &Value,
) -> Result<Value, ManagerError> {
    let id = payload["providerId"].as_str().unwrap_or("");
    let mut provider = find_account(paths, id)?;
    let previous_provider = provider.clone();
    if provider["enabled"] == false {
        return Err(error("Google 账号已禁用，请先恢复"));
    }
    let model = payload["model"].as_str().unwrap_or("");
    if !model.is_empty()
        && model
            != provider["runtimeConfig"]["mainModel"]
                .as_str()
                .unwrap_or("")
        && !provider["google"]["models"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|item| item["id"] == model)
    {
        return Err(error("请选择 Google 返回的可用模型"));
    }
    let proxy_url = payload["proxy"].as_str().unwrap_or("").trim();
    proxy::http_client(proxy_url)?;
    provider["google"]["proxy"] = json!(proxy_url);
    provider["runtimeConfig"]["mainModel"] = json!(model);
    write_provider(paths, &provider)?;
    let mut profiles = provider_store::read_profiles(paths)?;
    let previous_profiles = profiles.clone();
    for profile in &mut profiles {
        if profile["providerId"] == id {
            profile["model"] = json!(model);
        }
    }
    provider_store::write_profiles(paths, &profiles)?;
    if let Err(failure) =
        runtime_provider::sync_active_provider_config(paths, id, cli_targets).await
    {
        write_provider(paths, &previous_provider)?;
        provider_store::write_profiles(paths, &previous_profiles)?;
        return Err(failure);
    }
    Ok(provider)
}

pub(crate) fn update_metadata(
    paths: &AppPaths,
    id: &str,
    patch: Value,
) -> Result<(), ManagerError> {
    let mut provider = find_account(paths, id)?;
    for (key, value) in patch.as_object().unwrap() {
        provider["google"][key] = value.clone();
    }
    write_provider(paths, &provider)
}

fn write_provider(paths: &AppPaths, provider: &Value) -> Result<(), ManagerError> {
    let connection = database::open(paths)?;
    let count = connection.execute(
        "UPDATE providers SET payload_json=?2 WHERE item_key=?1",
        params![provider["id"].as_str().unwrap(), provider.to_string()],
    )?;
    if count == 0 {
        return Err(error("Google 账号已删除"));
    }
    Ok(())
}

async fn token_json(response: reqwest::Response) -> Result<Value, ManagerError> {
    let status = response.status();
    let value: Value = response
        .json()
        .await
        .map_err(|_| error("Google Token 返回格式无效"))?;
    if !status.is_success() {
        // 不把 Token 响应体或授权码写入错误、日志、前端状态。
        let code = value["error"].as_str().unwrap_or("oauth_error");
        let code = match code {
            "invalid_grant" | "invalid_client" | "unauthorized_client" | "access_denied" => code,
            _ => "oauth_error",
        };
        return Err(error(&format!(
            "Google 授权失败：{code}（HTTP {}），请重新登录或检查 OAuth 客户端配置",
            status.as_u16()
        )));
    }
    if value["access_token"].as_str().unwrap_or("").is_empty() {
        return Err(error("Google Token 响应缺少 access_token，请重新登录"));
    }
    Ok(value)
}

pub(crate) async fn json_response(response: reqwest::Response) -> Result<Value, ManagerError> {
    let status = response.status();
    let value: Value = response
        .json()
        .await
        .map_err(|_| error("Google 返回内容不是有效 JSON"))?;
    if !status.is_success() {
        return Err(error(&format!(
            "Google 返回 HTTP {}：{}",
            status.as_u16(),
            value["error"]["message"].as_str().unwrap_or("请求失败")
        )));
    }
    Ok(value)
}

pub(crate) async fn start_enabled(paths: &AppPaths) -> Result<(), ManagerError> {
    let profiles = provider_store::read_profiles(paths)?;
    let proxy_state = proxy::read_proxy_state(paths, "codex")?;
    for provider in provider_store::read_providers(paths)? {
        if provider["type"] != "google-account" || provider["enabled"] == false {
            continue;
        }
        let active = profiles
            .iter()
            .any(|profile| profile["providerId"] == provider["id"]);
        let pooled = proxy_state["enabled"] == true
            && (proxy_state["activeProviderId"] == provider["id"]
                || proxy_state["failoverProviderIds"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|id| id == &provider["id"]));
        if active || pooled {
            google_gateway::ensure_started(paths, &provider).await?;
        }
    }
    Ok(())
}

fn error(message: &str) -> ManagerError {
    ManagerError::System(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn google_quota_uses_matching_stricter_window_and_keeps_unknown_unknown() {
        let models = parse_models(&json!({"models": {
            "claude-sonnet-4-5": {"quotaInfo": {"remainingFraction": 0.8, "resetTime": "short"}},
            "gemini-3-flash": {"quotaInfo": {}},
            "old": {}
        }, "deprecatedModelIds": {"old": {"newModelId": "gemini-3-flash"}}}), &json!({"groups": [{"buckets": [
            {"bucketId": "claude_sonnet", "remainingFraction": 0.2, "resetTime": "weekly", "window": "WINDOW_WEEKLY"},
            {"bucketId": "unrelated", "remainingFraction": 0.0}
        ]}]})).unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["remainingPercent"], 20.0);
        assert_eq!(models[0]["resetTime"], "weekly");
        assert!(models[1]["remainingPercent"].is_null());
    }

    #[test]
    fn google_oauth_tokens_are_encrypted_and_not_in_public_provider() {
        let root =
            std::env::temp_dir().join(format!("google-account-test-{}", uuid::Uuid::new_v4()));
        let paths = crate::core::paths::resolve_app_paths(&root);
        let provider = json!({"id": "google-test", "type": "google-account", "cli": "codex", "google": {"email": "test@example.com"}});
        let tokens = json!({"access_token": "private-access", "refresh_token": "private-refresh"});
        save_login(&paths, &provider, &tokens).unwrap();
        assert_eq!(read_tokens(&paths, "google-test").unwrap(), tokens);
        let public = runtime_provider::read_public_providers(&paths)
            .unwrap()
            .to_string();
        let stored = json!(provider_store::read_keys(&paths).unwrap()).to_string();
        for value in [public, stored] {
            assert!(!value.contains("private-access") && !value.contains("private-refresh"));
        }
        assert!(runtime_provider::read_provider_key_value(
            &paths,
            &json!({"providerId": "google-oauth:google-test", "keyId": "default"})
        )
        .is_err());
        // 加密备份恢复后密钥采用多 Key 格式，Google 凭据仍然可以读取。
        let mut keys = provider_store::read_keys(&paths).unwrap();
        runtime_provider::set_provider_keys(
            &mut keys,
            "google-oauth:google-test",
            &[json!({"id": "default", "apiKey": tokens.to_string()})],
            "default".into(),
        )
        .unwrap();
        provider_store::write_keys(&paths, &keys).unwrap();
        assert_eq!(read_tokens(&paths, "google-test").unwrap(), tokens);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn google_oauth_ignores_wrong_state_and_accepts_valid_callback() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap();
                let task = tokio::spawn(receive_code(listener, "expected-state".into()));
                let client = reqwest::Client::builder().no_proxy().build().unwrap();
                let wrong = client
                    .get(format!(
                        "http://{address}/oauth-callback?state=wrong&code=secret"
                    ))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(wrong.status(), 400);
                let valid = client
                    .get(format!(
                        "http://{address}/oauth-callback?state=expected-state&code=accepted-code"
                    ))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(valid.status(), 200);
                assert_eq!(task.await.unwrap().unwrap(), "accepted-code");
            });
    }
}

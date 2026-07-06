use crate::core::error::ManagerError;
use bytes::Bytes;
use http::{Method, StatusCode};
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

const INDEX_HTML: &str = include_str!("../../toolbox-panel/index.html");
const STYLE_CSS: &str = include_str!("../../toolbox-panel/styles.css");
const APP_JS: &str = include_str!("../../toolbox-panel/app.js");
const TOOL_REGISTRY_JS: &str = include_str!("../../toolbox-panel/tools/registry.js");
const IMAGE_LINK_EXTRACTOR_JS: &str =
    include_str!("../../toolbox-panel/tools/image-link-extractor.js");
const STRING_DIFF_JS: &str = include_str!("../../toolbox-panel/tools/string-diff.js");

pub struct ToolboxServerRegistry {
    runtime: Option<ToolboxServerRuntime>,
}

struct ToolboxServerRuntime {
    url: String,
    _handle: JoinHandle<()>,
}

impl ToolboxServerRegistry {
    pub fn new() -> Self {
        Self { runtime: None }
    }
}

pub async fn open_toolbox(
    app: &AppHandle,
    registry: &mut ToolboxServerRegistry,
) -> Result<Value, ManagerError> {
    let url = ensure_toolbox_server(registry).await?;

    app.opener()
        .open_url(url.clone(), None::<&str>)
        .map_err(|error| ManagerError::System(error.to_string()))?;

    Ok(json!({
      "url": url
    }))
}

async fn ensure_toolbox_server(
    registry: &mut ToolboxServerRegistry,
) -> Result<String, ManagerError> {
    if let Some(runtime) = &registry.runtime {
        if !runtime._handle.is_finished() {
            return Ok(runtime.url.clone());
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let port = listener.local_addr()?.port();
    let url = format!("http://127.0.0.1:{}/", port);
    let handle = tokio::spawn(async move {
        loop {
            let Ok((stream, _addr)) = listener.accept().await else {
                break;
            };

            tokio::spawn(async move {
                let io = TokioIo::new(stream);
                if let Err(error) = http1::Builder::new()
                    .serve_connection(io, service_fn(handle_toolbox_request))
                    .await
                {
                    eprintln!("[实用工具服务] 请求处理失败: {}", error);
                }
            });
        }
    });

    registry.runtime = Some(ToolboxServerRuntime {
        url: url.clone(),
        _handle: handle,
    });

    Ok(url)
}

async fn handle_toolbox_request(
    request: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, std::convert::Infallible> {
    if request.method() != Method::GET {
        return Ok(response(
            StatusCode::METHOD_NOT_ALLOWED,
            "text/plain; charset=utf-8",
            "仅支持 GET 请求",
        ));
    }

    let path = request.uri().path();
    let response = match path {
        "/" | "/index.html" => response(StatusCode::OK, "text/html; charset=utf-8", INDEX_HTML),
        "/styles.css" => response(StatusCode::OK, "text/css; charset=utf-8", STYLE_CSS),
        "/app.js" => response(StatusCode::OK, "text/javascript; charset=utf-8", APP_JS),
        "/tools/registry.js" => response(
            StatusCode::OK,
            "text/javascript; charset=utf-8",
            TOOL_REGISTRY_JS,
        ),
        "/tools/image-link-extractor.js" => response(
            StatusCode::OK,
            "text/javascript; charset=utf-8",
            IMAGE_LINK_EXTRACTOR_JS,
        ),
        "/tools/string-diff.js" => response(
            StatusCode::OK,
            "text/javascript; charset=utf-8",
            STRING_DIFF_JS,
        ),
        _ => response(
            StatusCode::NOT_FOUND,
            "text/plain; charset=utf-8",
            "未找到工具资源",
        ),
    };

    Ok(response)
}

fn response(status: StatusCode, content_type: &str, body: &str) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header("content-type", content_type)
        .header("cache-control", "no-store")
        .body(Full::new(Bytes::copy_from_slice(body.as_bytes())))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::new())))
}

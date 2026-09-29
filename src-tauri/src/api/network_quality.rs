use crate::core::error::ManagerError;
use chrono::{SecondsFormat, Utc};
use futures_util::{
    future::{join, join5, join_all},
    StreamExt,
};
use reqwest::{header::LOCATION, Client, Method};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::path::Path;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, OnceLock,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};
use tokio::net::{lookup_host, TcpSocket};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

const TOOL_NAME: &str = "Monkey Thief Network Quality";
const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(4);
const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const IP_INTELLIGENCE_CACHE_TTL: Duration = Duration::from_secs(10 * 60);
const TCP_CONNECT_TIMEOUT: Duration = Duration::from_millis(2500);
const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const TCP_ROUNDS: usize = 3;
const BROWSER_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36";

static ACTIVE_RUNS: OnceLock<Mutex<HashMap<String, CancellationToken>>> = OnceLock::new();
static IP_INTELLIGENCE_CACHE: OnceLock<Mutex<HashMap<IpAddr, (Instant, IpFamilyIntelligence)>>> =
    OnceLock::new();

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum FamilySelection {
    Auto,
    Ipv4,
    Ipv6,
}

impl FamilySelection {
    fn includes(self, family: AddressFamily) -> bool {
        matches!(self, Self::Auto)
            || matches!((self, family), (Self::Ipv4, AddressFamily::Ipv4))
            || matches!((self, family), (Self::Ipv6, AddressFamily::Ipv6))
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Ipv4 => "ipv4",
            Self::Ipv6 => "ipv6",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AddressFamily {
    Ipv4,
    Ipv6,
}

impl AddressFamily {
    fn unspecified(self) -> IpAddr {
        match self {
            Self::Ipv4 => IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            Self::Ipv6 => IpAddr::V6(Ipv6Addr::UNSPECIFIED),
        }
    }

    fn matches(self, address: IpAddr) -> bool {
        matches!(
            (self, address),
            (Self::Ipv4, IpAddr::V4(_)) | (Self::Ipv6, IpAddr::V6(_))
        )
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Ipv4 => "ipv4",
            Self::Ipv6 => "ipv6",
        }
    }
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ModuleSelection {
    geo: bool,
    portal: bool,
    access: bool,
    ai: bool,
    path: bool,
}

impl ModuleSelection {
    fn any(self) -> bool {
        self.geo || self.portal || self.access || self.ai || self.path
    }

    fn count(self) -> usize {
        [self.geo, self.portal, self.access, self.ai, self.path]
            .into_iter()
            .filter(|enabled| *enabled)
            .count()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunPayload {
    run_id: String,
    family: FamilySelection,
    mask_ip: bool,
    modules: ModuleSelection,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunIdPayload {
    run_id: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DemoPayload {
    #[serde(default)]
    mask_ip: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExportPayload {
    target_path: String,
    report: Value,
    #[serde(default)]
    mask_ip: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TransportReport {
    family: String,
    mask_ip: bool,
    resolver: String,
    proxy: String,
}

#[derive(Clone, Default, Serialize)]
struct IdentityReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    ipv4: Option<IdentityFamily>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ipv6: Option<IdentityFamily>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IdentityFamily {
    address: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    asn: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    isp: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    organization: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    country_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    longitude: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timezone: Option<String>,
    source_votes: usize,
    attribution_source_count: usize,
}

#[derive(Clone, Default, Serialize)]
struct IpIntelligenceReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    ipv4: Option<IpFamilyIntelligence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ipv6: Option<IpFamilyIntelligence>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IpFamilyIntelligence {
    ip: String,
    sources: Vec<IpProviderResult>,
    consensus: IpConsensus,
    facts: Vec<IpFact>,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct IpIntelligenceData {
    country_code: Option<String>,
    country_name: Option<String>,
    continent_code: Option<String>,
    continent_name: Option<String>,
    region: Option<String>,
    city: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: Option<String>,
    registered_country_code: Option<String>,
    asn: Option<String>,
    as_domain: Option<String>,
    isp: Option<String>,
    organization: Option<String>,
    announced_prefix: Option<String>,
    ptr: Option<String>,
    is_announced: Option<bool>,
    origin_asns: Vec<String>,
    origin_holders: Vec<String>,
    rir: Option<String>,
    allocation_cidr: Option<String>,
    allocation_date: Option<String>,
    abuse_contact: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IpProviderResult {
    id: String,
    name: String,
    category: String,
    source_url: String,
    status: String,
    duration_ms: f64,
    data: Option<IpIntelligenceData>,
    error: Option<String>,
}

#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct IpConsensus {
    country_code: Option<String>,
    country_name: Option<String>,
    region: Option<String>,
    city: Option<String>,
    latitude: Option<f64>,
    longitude: Option<f64>,
    timezone: Option<String>,
    registered_country_code: Option<String>,
    asn: Option<String>,
    as_domain: Option<String>,
    isp: Option<String>,
    organization: Option<String>,
    announced_prefix: Option<String>,
    ptr: Option<String>,
    is_announced: Option<bool>,
    origin_asns: Vec<String>,
    origin_holders: Vec<String>,
    rir: Option<String>,
    allocation_cidr: Option<String>,
    allocation_date: Option<String>,
    abuse_contact: Option<String>,
    source_count: usize,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IpFact {
    key: String,
    values: Vec<IpFactValue>,
    source_count: usize,
    conflict: bool,
}

#[derive(Clone, Serialize)]
struct IpFactValue {
    value: String,
    sources: Vec<String>,
}

#[derive(Clone, Serialize)]
struct Finding {
    id: String,
    title: String,
    severity: String,
    detail: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CountryConsensus {
    code: String,
    name: String,
    count: usize,
    total: usize,
    percent: f64,
}

#[derive(Clone, Default, Serialize)]
struct ConsensusReport {
    ipv4: Vec<CountryConsensus>,
    ipv6: Vec<CountryConsensus>,
}

#[derive(Clone, Serialize)]
struct GeoResult {
    id: String,
    name: String,
    group: String,
    kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    ipv4: Option<GeoOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ipv6: Option<GeoOutcome>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct GeoOutcome {
    state: String,
    value: String,
    country_name: String,
    rtt_ms: f64,
    error: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectivityChecks {
    clean: bool,
    plain_http_blocked: bool,
    results: Vec<PortalResult>,
}

impl Default for ConnectivityChecks {
    fn default() -> Self {
        Self {
            clean: false,
            plain_http_blocked: false,
            results: Vec::new(),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PortalResult {
    id: String,
    name: String,
    vendor: String,
    url: String,
    verdict: String,
    status: u16,
    rtt_ms: f64,
    detail: String,
    error: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct EndpointResult {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    vendor: Option<String>,
    state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    region: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    http_status: Option<u16>,
    detail: String,
    rtt_ms: f64,
    error: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PathCapability {
    icmp: bool,
    raw: bool,
    path_visible: bool,
    hint: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PathVerdict {
    class: String,
    score: u8,
    rtt_ms: f64,
    jitter_ms: f64,
    loss: f64,
    hop_count: u8,
    notes: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PathTargetResult {
    id: String,
    name: String,
    host: String,
    network: String,
    method: String,
    resolved_ip: String,
    verdict: PathVerdict,
    error: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectivityReport {
    score: u8,
    grade: String,
    floor_ms: f64,
    median_rtt_ms: f64,
    capability: PathCapability,
    targets: Vec<PathTargetResult>,
}

impl Default for ConnectivityReport {
    fn default() -> Self {
        Self {
            score: 0,
            grade: "N/A".to_string(),
            floor_ms: 0.0,
            median_rtt_ms: 0.0,
            capability: tcp_capability(),
            targets: Vec::new(),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunReport {
    schema: u8,
    status: String,
    tool: String,
    timestamp: String,
    duration_ms: u64,
    transport: TransportReport,
    identity: IdentityReport,
    ip_intelligence: IpIntelligenceReport,
    findings: Vec<Finding>,
    consensus: ConsensusReport,
    geo: Vec<GeoResult>,
    connectivity_checks: ConnectivityChecks,
    service_access: Vec<EndpointResult>,
    ai_endpoints: Vec<EndpointResult>,
    connectivity: ConnectivityReport,
}

struct FamilyClients {
    follow_redirects: Client,
    no_redirects: Client,
}

struct NetworkStack {
    ipv4: Option<FamilyClients>,
    ipv6: Option<FamilyClients>,
    metadata: Client,
}

impl NetworkStack {
    fn build(selection: FamilySelection) -> Result<Self, ManagerError> {
        Ok(Self {
            ipv4: selection
                .includes(AddressFamily::Ipv4)
                .then(|| build_family_clients(AddressFamily::Ipv4))
                .transpose()?,
            ipv6: selection
                .includes(AddressFamily::Ipv6)
                .then(|| build_family_clients(AddressFamily::Ipv6))
                .transpose()?,
            metadata: build_metadata_client()?,
        })
    }

    fn client(&self, family: AddressFamily, no_redirects: bool) -> &Client {
        let clients = match family {
            AddressFamily::Ipv4 => self.ipv4.as_ref(),
            AddressFamily::Ipv6 => self.ipv6.as_ref(),
        }
        .expect("仅为已选择的地址族请求客户端");

        if no_redirects {
            &clients.no_redirects
        } else {
            &clients.follow_redirects
        }
    }

    fn metadata_client(&self) -> &Client {
        &self.metadata
    }
}

#[derive(Clone, Copy)]
enum IpProviderKind {
    IpWho,
    IpApiIs,
    GeoJs,
    Rdap,
}

#[derive(Clone, Copy)]
struct IpProviderEndpoint {
    id: &'static str,
    name: &'static str,
    category: &'static str,
    kind: IpProviderKind,
}

const IP_PROVIDER_ENDPOINTS: [IpProviderEndpoint; 3] = [
    IpProviderEndpoint {
        id: "ipwho",
        name: "ipwho.is",
        category: "geo",
        kind: IpProviderKind::IpWho,
    },
    IpProviderEndpoint {
        id: "geojs",
        name: "GeoJS",
        category: "network",
        kind: IpProviderKind::GeoJs,
    },
    IpProviderEndpoint {
        id: "rdap",
        name: "RDAP",
        category: "registry",
        kind: IpProviderKind::Rdap,
    },
];

const IPAPI_IS_ENDPOINT: IpProviderEndpoint = IpProviderEndpoint {
    id: "ipapi_is",
    name: "ipapi.is",
    category: "geo",
    kind: IpProviderKind::IpApiIs,
};

#[derive(Clone)]
struct ProgressEmitter {
    app: AppHandle,
    run_id: Arc<str>,
    completed: Arc<AtomicUsize>,
    total: usize,
}

impl ProgressEmitter {
    fn new(app: &AppHandle, run_id: &str, total: usize) -> Self {
        Self {
            app: app.clone(),
            run_id: Arc::from(run_id),
            completed: Arc::new(AtomicUsize::new(0)),
            total,
        }
    }

    fn emit(&self, phase: &str, message: &str, completed: usize) {
        let percent =
            ((completed.min(self.total) as f64 / self.total.max(1) as f64) * 100.0).round() as u8;
        let _ = self.app.emit(
            "network-quality:progress",
            json!({
                "runId": &*self.run_id,
                "phase": phase,
                "message": message,
                "completed": completed,
                "total": self.total,
                "percent": percent
            }),
        );
    }

    fn start(&self) {
        self.emit("starting", "正在初始化网络检测", 0);
    }

    fn complete(&self, phase: &str, message: &str) {
        let completed = self.completed.fetch_add(1, Ordering::Relaxed) + 1;
        self.emit(phase, message, completed);
    }
}

#[derive(Clone, Copy)]
enum GeoParser {
    CloudflareTrace,
    IpWho,
    CountryIs,
    IpApiCo,
}

#[derive(Clone, Copy)]
struct GeoEndpoint {
    id: &'static str,
    name: &'static str,
    url: &'static str,
    parser: GeoParser,
}

const GEO_ENDPOINTS: [GeoEndpoint; 4] = [
    GeoEndpoint {
        id: "cloudflare",
        name: "Cloudflare",
        url: "https://www.cloudflare.com/cdn-cgi/trace",
        parser: GeoParser::CloudflareTrace,
    },
    GeoEndpoint {
        id: "ipwho",
        name: "ipwho.is",
        url: "https://ipwho.is/",
        parser: GeoParser::IpWho,
    },
    GeoEndpoint {
        id: "country_is",
        name: "country.is",
        url: "https://api.country.is/",
        parser: GeoParser::CountryIs,
    },
    GeoEndpoint {
        id: "ipapi_co",
        name: "ipapi.co",
        url: "https://ipapi.co/json/",
        parser: GeoParser::IpApiCo,
    },
];

#[derive(Clone, Copy)]
enum BodyExpectation {
    Empty,
    Exact(&'static str),
}

#[derive(Clone, Copy)]
struct PortalEndpoint {
    id: &'static str,
    name: &'static str,
    vendor: &'static str,
    url: &'static str,
    expected_status: u16,
    body: BodyExpectation,
}

const PORTAL_ENDPOINTS: [PortalEndpoint; 6] = [
    PortalEndpoint {
        id: "google",
        name: "Google Connectivity Check",
        vendor: "Google",
        url: "http://connectivitycheck.gstatic.com/generate_204",
        expected_status: 204,
        body: BodyExpectation::Empty,
    },
    PortalEndpoint {
        id: "cloudflare",
        name: "Cloudflare Connectivity Check",
        vendor: "Cloudflare",
        url: "http://cp.cloudflare.com/generate_204",
        expected_status: 204,
        body: BodyExpectation::Empty,
    },
    PortalEndpoint {
        id: "microsoft",
        name: "Microsoft Connect Test",
        vendor: "Microsoft",
        url: "http://www.msftconnecttest.com/connecttest.txt",
        expected_status: 200,
        body: BodyExpectation::Exact("Microsoft Connect Test"),
    },
    PortalEndpoint {
        id: "firefox",
        name: "Firefox Portal Check",
        vendor: "Mozilla",
        url: "http://detectportal.firefox.com/success.txt",
        expected_status: 200,
        body: BodyExpectation::Exact("success"),
    },
    PortalEndpoint {
        id: "apple",
        name: "Apple Hotspot Detect",
        vendor: "Apple",
        url: "http://captive.apple.com/hotspot-detect.html",
        expected_status: 200,
        body: BodyExpectation::Exact(
            "<HTML><HEAD><TITLE>Success</TITLE></HEAD><BODY>Success</BODY></HTML>",
        ),
    },
    PortalEndpoint {
        id: "google_tls",
        name: "Google Connectivity Check TLS",
        vendor: "Google",
        url: "https://connectivitycheck.gstatic.com/generate_204",
        expected_status: 204,
        body: BodyExpectation::Empty,
    },
];

#[derive(Clone, Copy)]
enum AccessKind {
    ChatGpt,
    Claude,
    Gemini,
    YoutubePremium,
}

#[derive(Clone, Copy)]
struct AccessEndpoint {
    id: &'static str,
    name: &'static str,
    vendor: &'static str,
    url: &'static str,
    kind: AccessKind,
}

const ACCESS_ENDPOINTS: [AccessEndpoint; 4] = [
    AccessEndpoint {
        id: "chatgpt_web",
        name: "ChatGPT",
        vendor: "OpenAI",
        url: "https://api.openai.com/compliance/cookie_requirements",
        kind: AccessKind::ChatGpt,
    },
    AccessEndpoint {
        id: "claude_access",
        name: "Claude",
        vendor: "Anthropic",
        url: "https://claude.ai/",
        kind: AccessKind::Claude,
    },
    AccessEndpoint {
        id: "gemini_access",
        name: "Gemini",
        vendor: "Google",
        url: "https://gemini.google.com/app",
        kind: AccessKind::Gemini,
    },
    AccessEndpoint {
        id: "youtube_premium_access",
        name: "YouTube Premium",
        vendor: "Google",
        url: "https://www.youtube.com/premium",
        kind: AccessKind::YoutubePremium,
    },
];

#[derive(Clone, Copy)]
struct AiEndpoint {
    id: &'static str,
    name: &'static str,
    vendor: &'static str,
    url: &'static str,
    auth_statuses: &'static [u16],
    headers: &'static [(&'static str, &'static str)],
}

const NO_HEADERS: &[(&str, &str)] = &[];
const ANTHROPIC_HEADERS: &[(&str, &str)] = &[("anthropic-version", "2023-06-01")];
const AUTH_401: &[u16] = &[401];
const AUTH_401_403: &[u16] = &[401, 403];

const AI_ENDPOINTS: [AiEndpoint; 8] = [
    AiEndpoint {
        id: "openai",
        name: "OpenAI API",
        vendor: "OpenAI",
        url: "https://api.openai.com/v1/models",
        auth_statuses: AUTH_401,
        headers: NO_HEADERS,
    },
    AiEndpoint {
        id: "anthropic",
        name: "Anthropic API",
        vendor: "Anthropic",
        url: "https://api.anthropic.com/v1/models",
        auth_statuses: AUTH_401,
        headers: ANTHROPIC_HEADERS,
    },
    AiEndpoint {
        id: "gemini",
        name: "Gemini API",
        vendor: "Google",
        url: "https://generativelanguage.googleapis.com/v1beta/models",
        auth_statuses: AUTH_401_403,
        headers: NO_HEADERS,
    },
    AiEndpoint {
        id: "deepseek",
        name: "DeepSeek API",
        vendor: "DeepSeek",
        url: "https://api.deepseek.com/models",
        auth_statuses: AUTH_401,
        headers: NO_HEADERS,
    },
    AiEndpoint {
        id: "qwen_intl",
        name: "Qwen International API",
        vendor: "Alibaba Cloud",
        url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1/models",
        auth_statuses: AUTH_401,
        headers: NO_HEADERS,
    },
    AiEndpoint {
        id: "qwen_cn",
        name: "Qwen China API",
        vendor: "Alibaba Cloud",
        url: "https://dashscope.aliyuncs.com/compatible-mode/v1/models",
        auth_statuses: AUTH_401,
        headers: NO_HEADERS,
    },
    AiEndpoint {
        id: "moonshot",
        name: "Moonshot API",
        vendor: "Moonshot AI",
        url: "https://api.moonshot.cn/v1/models",
        auth_statuses: AUTH_401,
        headers: NO_HEADERS,
    },
    AiEndpoint {
        id: "zhipu",
        name: "Zhipu API",
        vendor: "Zhipu AI",
        url: "https://open.bigmodel.cn/api/paas/v4/models",
        auth_statuses: AUTH_401,
        headers: NO_HEADERS,
    },
];

#[derive(Clone, Copy)]
struct PathTarget {
    id: &'static str,
    name: &'static str,
    host: &'static str,
    network: &'static str,
    port: u16,
}

const PATH_TARGETS: [PathTarget; 5] = [
    PathTarget {
        id: "cloudflare",
        name: "Cloudflare",
        host: "cloudflare.com",
        network: "AS13335 / Anycast",
        port: 443,
    },
    PathTarget {
        id: "google",
        name: "Google",
        host: "google.com",
        network: "AS15169 / Anycast",
        port: 443,
    },
    PathTarget {
        id: "github",
        name: "GitHub",
        host: "github.com",
        network: "GitHub / CDN",
        port: 443,
    },
    PathTarget {
        id: "openai",
        name: "OpenAI",
        host: "api.openai.com",
        network: "OpenAI / CDN",
        port: 443,
    },
    PathTarget {
        id: "quad9",
        name: "Quad9",
        host: "dns.quad9.net",
        network: "AS19281 / Anycast",
        port: 443,
    },
];

#[derive(Clone)]
struct HttpObservation {
    status: u16,
    body: String,
    location: String,
    rtt_ms: f64,
}

#[derive(Clone, Debug)]
struct ProbeFailure {
    message: String,
    cancelled: bool,
}

impl ProbeFailure {
    fn cancelled() -> Self {
        Self {
            message: "检测已取消".to_string(),
            cancelled: true,
        }
    }

    fn network(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            cancelled: false,
        }
    }
}

#[derive(Clone)]
struct RawPathTarget {
    target: PathTarget,
    resolved_ip: String,
    samples: Vec<f64>,
    attempts: usize,
    error: String,
    cancelled: bool,
}

pub async fn dispatch(
    app: &AppHandle,
    action: &str,
    payload: Value,
) -> Result<Value, ManagerError> {
    match action {
        "run" => run(app, payload).await,
        "cancel" => cancel(payload).await,
        "demo" => demo(payload),
        "export" => export(payload).await,
        _ => Err(ManagerError::UnknownChannel(format!(
            "network-quality:{action}"
        ))),
    }
}

async fn run(app: &AppHandle, payload: Value) -> Result<Value, ManagerError> {
    let options: RunPayload = serde_json::from_value(payload)
        .map_err(|error| ManagerError::System(format!("网络检测参数无效：{error}")))?;
    validate_run_id(&options.run_id)?;
    if !options.modules.any() {
        return Err(ManagerError::System(
            "网络检测至少需要启用一个模块".to_string(),
        ));
    }

    let stack = NetworkStack::build(options.family)?;
    let cancel = CancellationToken::new();
    {
        let mut active = active_runs().lock().await;
        if active.contains_key(&options.run_id) {
            return Err(ManagerError::System(
                "相同 runId 的网络检测正在运行".to_string(),
            ));
        }
        active.insert(options.run_id.clone(), cancel.clone());
    }

    let report = run_report(app, &stack, &options, cancel).await;
    active_runs().lock().await.remove(&options.run_id);
    serde_json::to_value(report).map_err(ManagerError::Json)
}

async fn cancel(payload: Value) -> Result<Value, ManagerError> {
    let payload: RunIdPayload = serde_json::from_value(payload)
        .map_err(|error| ManagerError::System(format!("取消参数无效：{error}")))?;
    validate_run_id(&payload.run_id)?;
    let token = active_runs().lock().await.get(&payload.run_id).cloned();
    let cancelled = token.is_some();
    if let Some(token) = token {
        token.cancel();
    }
    Ok(json!({ "runId": payload.run_id, "cancelled": cancelled }))
}

fn demo(payload: Value) -> Result<Value, ManagerError> {
    let payload: DemoPayload = serde_json::from_value(payload)
        .map_err(|error| ManagerError::System(format!("演示参数无效：{error}")))?;
    serde_json::to_value(demo_report(payload.mask_ip)).map_err(ManagerError::Json)
}

async fn export(payload: Value) -> Result<Value, ManagerError> {
    let payload: ExportPayload = serde_json::from_value(payload)
        .map_err(|error| ManagerError::System(format!("导出参数无效：{error}")))?;
    if payload.target_path.trim().is_empty() {
        return Err(ManagerError::System("请选择 JSON 导出位置".to_string()));
    }
    if !payload.report.is_object()
        || payload.report.get("schema").and_then(Value::as_u64) != Some(1)
    {
        return Err(ManagerError::System(
            "仅支持导出 schema=1 的网络检测报告".to_string(),
        ));
    }
    let mut report = payload.report;
    let report_already_masked = report
        .pointer("/transport/maskIp")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if payload.mask_ip || report_already_masked {
        mask_export_value(&mut report);
    }
    let content = format!("{}\n", serde_json::to_string_pretty(&report)?);
    tokio::fs::write(Path::new(&payload.target_path), content).await?;
    Ok(json!({
        "exported": true,
        "targetPath": payload.target_path
    }))
}

fn mask_export_value(report: &mut Value) {
    let replacements = [
        "/identity/ipv4/address",
        "/identity/ipv6/address",
        "/ipIntelligence/ipv4/ip",
        "/ipIntelligence/ipv6/ip",
    ]
    .iter()
    .filter_map(|pointer| {
        let original = report.pointer(pointer)?.as_str()?.to_string();
        let parsed = original.parse::<IpAddr>().ok()?;
        Some((original, mask_ip(&parsed.to_string())))
    })
    .collect::<Vec<_>>();
    if !replacements.is_empty() {
        mask_json_strings(report, &replacements);
    }
    for family in ["ipv4", "ipv6"] {
        if let Some(sources) = report
            .pointer_mut(&format!("/ipIntelligence/{family}/sources"))
            .and_then(Value::as_array_mut)
        {
            for source in sources {
                let homepage = source
                    .get("id")
                    .and_then(Value::as_str)
                    .map(provider_homepage)
                    .unwrap_or("");
                source["sourceUrl"] = Value::String(homepage.to_string());
            }
        }
    }
    if let Some(transport) = report.get_mut("transport").and_then(Value::as_object_mut) {
        transport.insert("maskIp".to_string(), Value::Bool(true));
    }
}

fn mask_json_strings(value: &mut Value, replacements: &[(String, String)]) {
    match value {
        Value::String(text) => {
            for (original, masked) in replacements {
                *text = text.replace(original, masked);
            }
        }
        Value::Array(values) => {
            for value in values {
                mask_json_strings(value, replacements);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                mask_json_strings(value, replacements);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => {}
    }
}

fn active_runs() -> &'static Mutex<HashMap<String, CancellationToken>> {
    ACTIVE_RUNS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn ip_intelligence_cache() -> &'static Mutex<HashMap<IpAddr, (Instant, IpFamilyIntelligence)>> {
    IP_INTELLIGENCE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn validate_run_id(run_id: &str) -> Result<(), ManagerError> {
    let valid = !run_id.is_empty()
        && run_id.len() <= 128
        && run_id.trim() == run_id
        && run_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(ManagerError::System(
            "runId 只能包含字母、数字、连字符和下划线，且长度为 1-128".to_string(),
        ))
    }
}

fn build_family_clients(family: AddressFamily) -> Result<FamilyClients, ManagerError> {
    Ok(FamilyClients {
        follow_redirects: build_http_client(family, false)?,
        no_redirects: build_http_client(family, true)?,
    })
}

fn build_metadata_client() -> Result<Client, ManagerError> {
    Client::builder()
        .no_proxy()
        .connect_timeout(HTTP_CONNECT_TIMEOUT)
        .timeout(HTTP_REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("Monkey-Thief-Network-Quality/1.0")
        .build()
        .map_err(|error| ManagerError::System(format!("创建 IP 情报客户端失败：{error}")))
}

fn build_http_client(family: AddressFamily, no_redirects: bool) -> Result<Client, ManagerError> {
    let redirect = if no_redirects {
        reqwest::redirect::Policy::none()
    } else {
        reqwest::redirect::Policy::limited(10)
    };
    Client::builder()
        .no_proxy()
        .local_address(family.unspecified())
        .connect_timeout(HTTP_CONNECT_TIMEOUT)
        .timeout(HTTP_REQUEST_TIMEOUT)
        .redirect(redirect)
        .user_agent("Monkey-Thief-Network-Quality/1.0")
        .build()
        .map_err(|error| ManagerError::System(format!("创建网络检测客户端失败：{error}")))
}

async fn run_report(
    app: &AppHandle,
    stack: &NetworkStack,
    options: &RunPayload,
    cancel: CancellationToken,
) -> RunReport {
    let started = Instant::now();
    let timestamp = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
    let progress = ProgressEmitter::new(app, &options.run_id, options.modules.count() + 1);
    progress.start();

    let (mut identity, ip_intelligence) = detect_identity(stack, options.family, &cancel).await;
    progress.complete("identity", "公网身份检测完成");
    let primary_family = choose_primary_family(options.family, &identity);

    let geo_progress = progress.clone();
    let geo_cancel = cancel.clone();
    let geo_future = async {
        if !options.modules.geo {
            return Vec::new();
        }
        let result = run_geo(stack, options.family, &identity, &geo_cancel).await;
        geo_progress.complete("geo", "地理位置共识检测完成");
        result
    };

    let portal_progress = progress.clone();
    let portal_cancel = cancel.clone();
    let portal_future = async {
        if !options.modules.portal {
            return ConnectivityChecks::default();
        }
        let result = run_portal(stack, primary_family, &portal_cancel).await;
        portal_progress.complete("portal", "系统连通性检测完成");
        result
    };

    let access_progress = progress.clone();
    let access_cancel = cancel.clone();
    let access_future = async {
        if !options.modules.access {
            return Vec::new();
        }
        let result = run_access(stack, primary_family, &access_cancel).await;
        access_progress.complete("access", "服务可用性检测完成");
        result
    };

    let ai_progress = progress.clone();
    let ai_cancel = cancel.clone();
    let ai_future = async {
        if !options.modules.ai {
            return Vec::new();
        }
        let result = run_ai(stack, primary_family, &ai_cancel).await;
        ai_progress.complete("ai", "AI API 可达性检测完成");
        result
    };

    let path_progress = progress.clone();
    let path_cancel = cancel.clone();
    let path_future = async {
        if !options.modules.path {
            return ConnectivityReport::default();
        }
        let result = run_path(primary_family, &path_cancel).await;
        path_progress.complete("path", "目标连接质量检测完成");
        result
    };

    let (geo, connectivity_checks, service_access, ai_endpoints, connectivity) = join5(
        geo_future,
        portal_future,
        access_future,
        ai_future,
        path_future,
    )
    .await;

    let consensus = ConsensusReport {
        ipv4: country_consensus(
            geo.iter()
                .filter_map(|result| result.ipv4.as_ref())
                .filter(|outcome| outcome.state == "success")
                .map(|outcome| outcome.value.as_str()),
        ),
        ipv6: country_consensus(
            geo.iter()
                .filter_map(|result| result.ipv6.as_ref())
                .filter(|outcome| outcome.state == "success")
                .map(|outcome| outcome.value.as_str()),
        ),
    };
    backfill_identity_from_geo_consensus(&mut identity, &consensus);
    let findings = build_findings(
        options,
        &identity,
        &connectivity_checks,
        cancel.is_cancelled(),
    );
    let identity_available = selected_identity_available(options.family, &identity);
    let status = if cancel.is_cancelled() {
        "cancelled"
    } else if !identity_available {
        "error"
    } else {
        "success"
    };
    let mut report = RunReport {
        schema: 1,
        status: status.to_string(),
        tool: TOOL_NAME.to_string(),
        timestamp,
        duration_ms: elapsed_ms(started),
        transport: TransportReport {
            family: options.family.as_str().to_string(),
            mask_ip: options.mask_ip,
            resolver: "system".to_string(),
            proxy: "direct".to_string(),
        },
        identity,
        ip_intelligence,
        findings,
        consensus,
        geo,
        connectivity_checks,
        service_access,
        ai_endpoints,
        connectivity,
    };
    if options.mask_ip {
        mask_report_identity(&mut report.identity);
        mask_report_intelligence(&mut report.ip_intelligence);
    }
    progress.emit(status, "网络检测结束", progress.total);
    report
}

fn backfill_identity_from_geo_consensus(
    identity: &mut IdentityReport,
    consensus: &ConsensusReport,
) {
    backfill_identity_family(identity.ipv4.as_mut(), &consensus.ipv4);
    backfill_identity_family(identity.ipv6.as_mut(), &consensus.ipv6);
}

fn backfill_identity_family(identity: Option<&mut IdentityFamily>, consensus: &[CountryConsensus]) {
    let (Some(identity), Some(country)) = (identity, consensus.first()) else {
        return;
    };
    if identity.country_code.is_none() {
        identity.country_code = Some(country.code.clone());
    }
    if identity.country.is_none() {
        identity.country = Some(country.name.clone());
    }
}

fn choose_primary_family(selection: FamilySelection, identity: &IdentityReport) -> AddressFamily {
    match selection {
        FamilySelection::Ipv4 => AddressFamily::Ipv4,
        FamilySelection::Ipv6 => AddressFamily::Ipv6,
        FamilySelection::Auto if identity.ipv4.is_none() && identity.ipv6.is_some() => {
            AddressFamily::Ipv6
        }
        FamilySelection::Auto => AddressFamily::Ipv4,
    }
}

fn selected_identity_available(selection: FamilySelection, identity: &IdentityReport) -> bool {
    match selection {
        FamilySelection::Auto => identity.ipv4.is_some() || identity.ipv6.is_some(),
        FamilySelection::Ipv4 => identity.ipv4.is_some(),
        FamilySelection::Ipv6 => identity.ipv6.is_some(),
    }
}

async fn detect_identity(
    stack: &NetworkStack,
    selection: FamilySelection,
    cancel: &CancellationToken,
) -> (IdentityReport, IpIntelligenceReport) {
    let ipv4 = async {
        if selection.includes(AddressFamily::Ipv4) {
            detect_identity_family(stack, AddressFamily::Ipv4, cancel).await
        } else {
            None
        }
    };
    let ipv6 = async {
        if selection.includes(AddressFamily::Ipv6) {
            detect_identity_family(stack, AddressFamily::Ipv6, cancel).await
        } else {
            None
        }
    };
    let (ipv4, ipv6) = join(ipv4, ipv6).await;
    let (ipv4_identity, ipv4_intelligence) = match ipv4 {
        Some((identity, intelligence)) => (Some(identity), Some(intelligence)),
        None => (None, None),
    };
    let (ipv6_identity, ipv6_intelligence) = match ipv6 {
        Some((identity, intelligence)) => (Some(identity), Some(intelligence)),
        None => (None, None),
    };
    (
        IdentityReport {
            ipv4: ipv4_identity,
            ipv6: ipv6_identity,
        },
        IpIntelligenceReport {
            ipv4: ipv4_intelligence,
            ipv6: ipv6_intelligence,
        },
    )
}

async fn detect_identity_family(
    stack: &NetworkStack,
    family: AddressFamily,
    cancel: &CancellationToken,
) -> Option<(IdentityFamily, IpFamilyIntelligence)> {
    const ECHO_ENDPOINTS: [&str; 5] = [
        "https://www.cloudflare.com/cdn-cgi/trace",
        "https://api64.ipify.org/",
        "https://ident.me/",
        "https://ifconfig.me/ip",
        "https://icanhazip.com/",
    ];

    let responses = join_all(ECHO_ENDPOINTS.iter().map(|url| async move {
        send_request(stack.client(family, false), Method::GET, url, &[], cancel)
            .await
            .ok()
            .filter(|response| (200..300).contains(&response.status))
            .and_then(|response| parse_echo_ip(&response.body, family))
    }))
    .await;
    let (address, source_votes) = select_public_ip(&responses, family)?;
    let intelligence = lookup_ip_intelligence(stack, address, cancel).await;
    let identity = identity_from_intelligence(address, source_votes, &intelligence);
    Some((identity, intelligence))
}

async fn lookup_ip_intelligence(
    stack: &NetworkStack,
    address: IpAddr,
    cancel: &CancellationToken,
) -> IpFamilyIntelligence {
    let now = Instant::now();
    {
        let mut cache = ip_intelligence_cache().lock().await;
        cache
            .retain(|_, (stored_at, _)| now.duration_since(*stored_at) < IP_INTELLIGENCE_CACHE_TTL);
        if let Some((_, report)) = cache.get(&address) {
            return report.clone();
        }
    }

    let generic = join_all(
        IP_PROVIDER_ENDPOINTS
            .iter()
            .map(|endpoint| probe_ip_provider(stack, address, *endpoint, cancel)),
    );
    let ripe = probe_ripestat(stack, address, cancel);
    let (mut sources, ripe) = join(generic, ripe).await;
    let has_complete_location = sources.iter().any(|source| {
        source.status == "ok"
            && source.data.as_ref().is_some_and(|data| {
                (data.country_code.is_some() || data.country_name.is_some())
                    && data.region.is_some()
                    && data.city.is_some()
            })
    });
    if !has_complete_location && !cancel.is_cancelled() {
        sources.push(probe_ip_provider(stack, address, IPAPI_IS_ENDPOINT, cancel).await);
    }
    sources.push(ripe);
    let consensus = build_ip_consensus(&sources);
    let facts = build_ip_facts(&sources);
    let report = IpFamilyIntelligence {
        ip: address.to_string(),
        sources,
        consensus,
        facts,
    };
    let has_location = (report.consensus.country_code.is_some()
        || report.consensus.country_name.is_some())
        && report.consensus.region.is_some()
        && report.consensus.city.is_some();
    if !cancel.is_cancelled() && has_location {
        ip_intelligence_cache()
            .lock()
            .await
            .insert(address, (Instant::now(), report.clone()));
    }
    report
}

async fn probe_ip_provider(
    stack: &NetworkStack,
    address: IpAddr,
    endpoint: IpProviderEndpoint,
    cancel: &CancellationToken,
) -> IpProviderResult {
    let source_url = match endpoint.kind {
        IpProviderKind::IpWho => format!("https://ipwho.is/{address}"),
        IpProviderKind::IpApiIs => format!("https://api.ipapi.is/?q={address}"),
        IpProviderKind::GeoJs => format!("https://get.geojs.io/v1/ip/geo/{address}.json"),
        IpProviderKind::Rdap => format!("https://rdap.org/ip/{address}"),
    };
    let started = Instant::now();
    let response = send_request(
        stack.metadata_client(),
        Method::GET,
        &source_url,
        &[],
        cancel,
    )
    .await;
    let duration_ms = round_ms(elapsed_ms_f64(started));
    let result =
        |status: &str, data: Option<IpIntelligenceData>, error: Option<String>| IpProviderResult {
            id: endpoint.id.to_string(),
            name: endpoint.name.to_string(),
            category: endpoint.category.to_string(),
            source_url: source_url.clone(),
            status: status.to_string(),
            duration_ms,
            data,
            error,
        };

    match response {
        Ok(response) if matches!(response.status, 204 | 404) => result("empty", None, None),
        Ok(response) if !(200..300).contains(&response.status) => {
            result("error", None, Some(format!("HTTP {}", response.status)))
        }
        Ok(response) => match serde_json::from_str::<Value>(&response.body) {
            Ok(value) => match parse_ip_provider_data(endpoint.kind, &value) {
                Some(data) if data.has_data() => result("ok", Some(data), None),
                _ => result("empty", None, None),
            },
            Err(_) => result("error", None, Some("供应商响应不是有效 JSON".to_string())),
        },
        Err(error) if error.cancelled => result("cancelled", None, Some(error.message)),
        Err(error) => result("error", None, Some(error.message)),
    }
}

async fn probe_ripestat(
    stack: &NetworkStack,
    address: IpAddr,
    cancel: &CancellationToken,
) -> IpProviderResult {
    let source_url = format!("https://stat.ripe.net/{address}");
    let prefix_url =
        format!("https://stat.ripe.net/data/prefix-overview/data.json?resource={address}");
    let reverse_url =
        format!("https://stat.ripe.net/data/reverse-dns-ip/data.json?resource={address}");
    let started = Instant::now();
    let (prefix, reverse) = join(
        send_request(
            stack.metadata_client(),
            Method::GET,
            &prefix_url,
            &[],
            cancel,
        ),
        send_request(
            stack.metadata_client(),
            Method::GET,
            &reverse_url,
            &[],
            cancel,
        ),
    )
    .await;

    let mut data = IpIntelligenceData::default();
    let mut errors = Vec::new();
    let mut cancelled = false;
    merge_ripestat_response(&mut data, prefix, true, &mut errors, &mut cancelled);
    merge_ripestat_response(&mut data, reverse, false, &mut errors, &mut cancelled);
    let has_data = data.has_data();
    IpProviderResult {
        id: "ripestat".to_string(),
        name: "RIPEstat".to_string(),
        category: "network".to_string(),
        source_url,
        status: if has_data {
            "ok"
        } else if cancelled {
            "cancelled"
        } else if errors.is_empty() {
            "empty"
        } else {
            "error"
        }
        .to_string(),
        duration_ms: round_ms(elapsed_ms_f64(started)),
        data: has_data.then_some(data),
        error: (!errors.is_empty()).then(|| errors.join("；")),
    }
}

impl IpIntelligenceData {
    fn has_data(&self) -> bool {
        self.country_code.is_some()
            || self.country_name.is_some()
            || self.continent_code.is_some()
            || self.continent_name.is_some()
            || self.region.is_some()
            || self.city.is_some()
            || self.latitude.is_some()
            || self.longitude.is_some()
            || self.timezone.is_some()
            || self.registered_country_code.is_some()
            || self.asn.is_some()
            || self.as_domain.is_some()
            || self.isp.is_some()
            || self.organization.is_some()
            || self.announced_prefix.is_some()
            || self.ptr.is_some()
            || self.is_announced.is_some()
            || !self.origin_asns.is_empty()
            || !self.origin_holders.is_empty()
            || self.rir.is_some()
            || self.allocation_cidr.is_some()
            || self.allocation_date.is_some()
            || self.abuse_contact.is_some()
    }
}

fn parse_ip_provider_data(kind: IpProviderKind, value: &Value) -> Option<IpIntelligenceData> {
    let data = match kind {
        IpProviderKind::IpWho => {
            if value.get("success").and_then(Value::as_bool) == Some(false) {
                return None;
            }
            IpIntelligenceData {
                country_code: value
                    .get("country_code")
                    .and_then(Value::as_str)
                    .and_then(normalize_country_code),
                country_name: non_empty_json_string(value.get("country")),
                continent_code: non_empty_json_string(value.get("continent_code")),
                continent_name: non_empty_json_string(value.get("continent")),
                region: non_empty_json_string(value.get("region")),
                city: non_empty_json_string(value.get("city")),
                latitude: value.get("latitude").and_then(json_f64),
                longitude: value.get("longitude").and_then(json_f64),
                timezone: first_non_empty_string(&[
                    value.pointer("/timezone/id"),
                    value.get("timezone"),
                ]),
                asn: value.pointer("/connection/asn").and_then(normalize_asn),
                as_domain: non_empty_json_string(value.pointer("/connection/domain")),
                isp: non_empty_json_string(value.pointer("/connection/isp")),
                organization: first_non_empty_string(&[
                    value.pointer("/connection/org"),
                    value.pointer("/connection/isp"),
                ]),
                ..IpIntelligenceData::default()
            }
        }
        IpProviderKind::IpApiIs => {
            if value.get("is_bogon").and_then(Value::as_bool) == Some(true) {
                return None;
            }
            let company = first_non_empty_string(&[
                value.get("company"),
                value.pointer("/company/name"),
                value.pointer("/asn/org"),
            ]);
            IpIntelligenceData {
                country_code: first_non_empty_string(&[
                    value.get("country_code"),
                    value.pointer("/location/country_code"),
                ])
                .as_deref()
                .and_then(normalize_country_code),
                country_name: first_non_empty_string(&[
                    value.get("country"),
                    value.pointer("/location/country"),
                ]),
                region: first_non_empty_string(&[
                    value.get("region"),
                    value.pointer("/location/state"),
                ]),
                city: first_non_empty_string(&[value.get("city"), value.pointer("/location/city")]),
                latitude: value
                    .get("lat")
                    .or_else(|| value.get("latitude"))
                    .or_else(|| value.pointer("/location/latitude"))
                    .and_then(json_f64),
                longitude: value
                    .get("lon")
                    .or_else(|| value.get("longitude"))
                    .or_else(|| value.pointer("/location/longitude"))
                    .and_then(json_f64),
                timezone: first_non_empty_string(&[
                    value.get("timezone"),
                    value.pointer("/location/timezone"),
                ]),
                asn: value
                    .get("asn")
                    .or_else(|| value.pointer("/asn/asn"))
                    .and_then(normalize_asn),
                isp: company.clone(),
                organization: company,
                announced_prefix: first_non_empty_string(&[
                    value.get("route"),
                    value.get("announced_prefix"),
                    value.pointer("/asn/route"),
                ]),
                abuse_contact: first_non_empty_string(&[
                    value.get("abuse_contact"),
                    value.pointer("/company/abuse_email"),
                ]),
                ..IpIntelligenceData::default()
            }
        }
        IpProviderKind::GeoJs => {
            let organization = non_empty_json_string(value.get("organization"));
            IpIntelligenceData {
                country_code: value
                    .get("country_code")
                    .and_then(Value::as_str)
                    .and_then(normalize_country_code),
                country_name: non_empty_json_string(value.get("country")),
                continent_code: non_empty_json_string(value.get("continent_code")),
                continent_name: non_empty_json_string(value.get("continent")),
                region: first_non_empty_string(&[value.get("region"), value.get("region_name")]),
                city: non_empty_json_string(value.get("city")),
                latitude: value.get("latitude").and_then(json_f64),
                longitude: value.get("longitude").and_then(json_f64),
                timezone: non_empty_json_string(value.get("timezone")),
                asn: value.get("asn").and_then(normalize_asn),
                isp: organization.clone(),
                organization,
                ..IpIntelligenceData::default()
            }
        }
        IpProviderKind::Rdap => parse_rdap_data(value),
    };
    data.has_data().then_some(data)
}

fn parse_rdap_data(value: &Value) -> IpIntelligenceData {
    let allocation_cidr = value
        .get("cidr0_cidrs")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .and_then(|item| {
            let prefix = first_non_empty_string(&[item.get("v4prefix"), item.get("v6prefix")])?;
            let length = item.get("length").and_then(json_integer)?;
            Some(format!("{prefix}/{length}"))
        });
    let allocation_date = value
        .get("events")
        .and_then(Value::as_array)
        .and_then(|events| {
            events.iter().find_map(|event| {
                (event.get("eventAction").and_then(Value::as_str) == Some("registration"))
                    .then(|| non_empty_json_string(event.get("eventDate")))
                    .flatten()
            })
        });
    IpIntelligenceData {
        registered_country_code: value
            .get("country")
            .and_then(Value::as_str)
            .and_then(normalize_country_code),
        organization: first_non_empty_string(&[value.get("name"), value.get("handle")]),
        rir: infer_rir(value),
        allocation_cidr,
        allocation_date,
        abuse_contact: find_rdap_abuse_email(value),
        ..IpIntelligenceData::default()
    }
}

fn infer_rir(value: &Value) -> Option<String> {
    let mut hints = Vec::new();
    if let Some(port43) = value.get("port43").and_then(Value::as_str) {
        hints.push(port43.to_ascii_lowercase());
    }
    if let Some(links) = value.get("links").and_then(Value::as_array) {
        hints.extend(
            links
                .iter()
                .filter_map(|link| link.get("href").and_then(Value::as_str))
                .map(str::to_ascii_lowercase),
        );
    }
    for (marker, rir) in [
        ("arin", "ARIN"),
        ("ripe", "RIPE NCC"),
        ("apnic", "APNIC"),
        ("lacnic", "LACNIC"),
        ("afrinic", "AFRINIC"),
    ] {
        if hints.iter().any(|hint| hint.contains(marker)) {
            return Some(rir.to_string());
        }
    }
    None
}

fn find_rdap_abuse_email(value: &Value) -> Option<String> {
    value
        .get("entities")
        .and_then(Value::as_array)
        .and_then(|entities| {
            entities.iter().find_map(|entity| {
                let is_abuse = entity
                    .get("roles")
                    .and_then(Value::as_array)
                    .is_some_and(|roles| {
                        roles
                            .iter()
                            .filter_map(Value::as_str)
                            .any(|role| role.eq_ignore_ascii_case("abuse"))
                    });
                if is_abuse {
                    let email = entity
                        .get("vcardArray")
                        .and_then(Value::as_array)
                        .and_then(|parts| parts.get(1))
                        .and_then(Value::as_array)
                        .and_then(|entries| {
                            entries.iter().find_map(|entry| {
                                let fields = entry.as_array()?;
                                (fields.first()?.as_str()? == "email")
                                    .then(|| fields.get(3).and_then(Value::as_str))
                                    .flatten()
                                    .map(str::trim)
                                    .filter(|email| !email.is_empty())
                                    .map(str::to_string)
                            })
                        });
                    if email.is_some() {
                        return email;
                    }
                }
                find_rdap_abuse_email(entity)
            })
        })
}

fn merge_ripestat_response(
    target: &mut IpIntelligenceData,
    response: Result<HttpObservation, ProbeFailure>,
    prefix: bool,
    errors: &mut Vec<String>,
    cancelled: &mut bool,
) {
    let response = match response {
        Ok(response) if (200..300).contains(&response.status) => response,
        Ok(response) => {
            errors.push(format!("RIPEstat HTTP {}", response.status));
            return;
        }
        Err(error) => {
            *cancelled |= error.cancelled;
            errors.push(error.message);
            return;
        }
    };
    let Ok(value) = serde_json::from_str::<Value>(&response.body) else {
        errors.push("RIPEstat 响应不是有效 JSON".to_string());
        return;
    };
    let parsed = if prefix {
        parse_ripestat_prefix(&value)
    } else {
        parse_ripestat_reverse(&value)
    };
    if let Some(parsed) = parsed {
        merge_ip_data(target, parsed);
    }
}

fn parse_ripestat_prefix(value: &Value) -> Option<IpIntelligenceData> {
    let data = value.get("data")?;
    let mut origin_asns = data
        .get("asns")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(normalize_asn)
        .collect::<Vec<_>>();
    origin_asns.sort();
    origin_asns.dedup();
    let holder = non_empty_json_string(data.get("holder"));
    let mut result = IpIntelligenceData {
        announced_prefix: non_empty_json_string(data.get("prefix")),
        is_announced: data
            .get("announced")
            .or_else(|| data.get("is_announced"))
            .and_then(Value::as_bool),
        asn: origin_asns.first().cloned(),
        organization: holder.clone(),
        origin_asns,
        origin_holders: holder.into_iter().collect(),
        ..IpIntelligenceData::default()
    };
    if result.is_announced.is_none()
        && result.announced_prefix.is_some()
        && !result.origin_asns.is_empty()
    {
        result.is_announced = Some(true);
    }
    result.has_data().then_some(result)
}

fn parse_ripestat_reverse(value: &Value) -> Option<IpIntelligenceData> {
    let ptr = [
        "/data/result/0",
        "/data/result/0/value",
        "/data/result/0/name",
        "/data/records/0/value",
        "/data/records/0/name",
        "/data/ptr_records/0",
    ]
    .iter()
    .find_map(|pointer| non_empty_json_string(value.pointer(pointer)));
    ptr.map(|ptr| IpIntelligenceData {
        ptr: Some(ptr.trim_end_matches('.').to_string()),
        ..IpIntelligenceData::default()
    })
}

fn merge_ip_data(target: &mut IpIntelligenceData, source: IpIntelligenceData) {
    macro_rules! fill {
        ($field:ident) => {
            if target.$field.is_none() {
                target.$field = source.$field;
            }
        };
    }
    fill!(country_code);
    fill!(country_name);
    fill!(continent_code);
    fill!(continent_name);
    fill!(region);
    fill!(city);
    fill!(latitude);
    fill!(longitude);
    fill!(timezone);
    fill!(registered_country_code);
    fill!(asn);
    fill!(as_domain);
    fill!(isp);
    fill!(organization);
    fill!(announced_prefix);
    fill!(ptr);
    fill!(is_announced);
    fill!(rir);
    fill!(allocation_cidr);
    fill!(allocation_date);
    fill!(abuse_contact);
    target.origin_asns.extend(source.origin_asns);
    target.origin_asns.sort();
    target.origin_asns.dedup();
    target.origin_holders.extend(source.origin_holders);
    target.origin_holders.sort();
    target.origin_holders.dedup();
}

fn build_ip_consensus(sources: &[IpProviderResult]) -> IpConsensus {
    let mut merged = IpIntelligenceData::default();
    let mut source_count = 0;
    for source in sources {
        if source.status != "ok" {
            continue;
        }
        let Some(data) = source.data.clone() else {
            continue;
        };
        source_count += 1;
        merge_ip_data(&mut merged, data);
    }
    IpConsensus {
        country_code: merged.country_code,
        country_name: merged.country_name,
        region: merged.region,
        city: merged.city,
        latitude: merged.latitude,
        longitude: merged.longitude,
        timezone: merged.timezone,
        registered_country_code: merged.registered_country_code,
        asn: merged.asn,
        as_domain: merged.as_domain,
        isp: merged.isp,
        organization: merged.organization,
        announced_prefix: merged.announced_prefix,
        ptr: merged.ptr,
        is_announced: merged.is_announced,
        origin_asns: merged.origin_asns,
        origin_holders: merged.origin_holders,
        rir: merged.rir,
        allocation_cidr: merged.allocation_cidr,
        allocation_date: merged.allocation_date,
        abuse_contact: merged.abuse_contact,
        source_count,
    }
}

fn build_ip_facts(sources: &[IpProviderResult]) -> Vec<IpFact> {
    let specifications: [(&str, fn(&IpIntelligenceData) -> Option<String>); 14] = [
        ("country", |data| {
            data.country_code
                .as_deref()
                .map(country_name)
                .or_else(|| data.country_name.clone())
        }),
        ("city", |data| data.city.clone()),
        ("region", |data| data.region.clone()),
        ("registration_country", |data| {
            data.registered_country_code.clone()
        }),
        ("rir", |data| data.rir.clone()),
        ("allocation", |data| data.allocation_cidr.clone()),
        ("asn", |data| data.asn.clone()),
        ("isp", |data| data.isp.clone()),
        ("organization", |data| data.organization.clone()),
        ("announced_prefix", |data| data.announced_prefix.clone()),
        ("ptr", |data| data.ptr.clone()),
        ("announcement", |data| {
            data.is_announced
                .map(|value| if value { "announced" } else { "not_announced" }.to_string())
        }),
        ("origin_asn", |data| join_fact_values(&data.origin_asns)),
        ("origin_holder", |data| {
            join_fact_values(&data.origin_holders)
        }),
    ];
    specifications
        .into_iter()
        .filter_map(|(key, extract)| build_ip_fact(key, sources, extract))
        .collect()
}

fn build_ip_fact(
    key: &str,
    sources: &[IpProviderResult],
    extract: fn(&IpIntelligenceData) -> Option<String>,
) -> Option<IpFact> {
    let mut values: Vec<IpFactValue> = Vec::new();
    for source in sources {
        if source.status != "ok" {
            continue;
        }
        let Some(value) = source
            .data
            .as_ref()
            .and_then(extract)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        else {
            continue;
        };
        if let Some(existing) = values
            .iter_mut()
            .find(|existing| existing.value.eq_ignore_ascii_case(&value))
        {
            existing.sources.push(source.id.clone());
        } else {
            values.push(IpFactValue {
                value,
                sources: vec![source.id.clone()],
            });
        }
    }
    if values.is_empty() {
        return None;
    }
    let source_count = values.iter().map(|value| value.sources.len()).sum();
    Some(IpFact {
        key: key.to_string(),
        conflict: values.len() > 1,
        values,
        source_count,
    })
}

fn join_fact_values(values: &[String]) -> Option<String> {
    (!values.is_empty()).then(|| {
        let mut values = values.to_vec();
        values.sort();
        values.dedup();
        values.join(", ")
    })
}

fn identity_from_intelligence(
    address: IpAddr,
    source_votes: usize,
    intelligence: &IpFamilyIntelligence,
) -> IdentityFamily {
    let consensus = &intelligence.consensus;
    IdentityFamily {
        address: address.to_string(),
        asn: consensus.asn.clone(),
        isp: consensus.isp.clone(),
        organization: consensus
            .organization
            .clone()
            .or_else(|| consensus.isp.clone()),
        country: consensus.country_name.clone(),
        country_code: consensus.country_code.clone(),
        region: consensus.region.clone(),
        city: consensus.city.clone(),
        latitude: consensus.latitude,
        longitude: consensus.longitude,
        timezone: consensus.timezone.clone(),
        source_votes,
        attribution_source_count: consensus.source_count,
    }
}

fn non_empty_json_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn json_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str()?.trim().parse().ok())
        .filter(|value| value.is_finite())
}

fn normalize_asn(value: &Value) -> Option<String> {
    if let Some(number) = value.as_u64() {
        return (number > 0).then(|| format!("AS{number}"));
    }
    let text = value.as_str()?.trim();
    let digits = text
        .strip_prefix("AS")
        .or_else(|| text.strip_prefix("as"))
        .unwrap_or(text)
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    let number = digits.parse::<u64>().ok()?;
    (number > 0).then(|| format!("AS{number}"))
}

async fn run_geo(
    stack: &NetworkStack,
    selection: FamilySelection,
    identity: &IdentityReport,
    cancel: &CancellationToken,
) -> Vec<GeoResult> {
    let ipv4 = async {
        if !selection.includes(AddressFamily::Ipv4) {
            return Vec::new();
        }
        let public_ip = identity
            .ipv4
            .as_ref()
            .map(|identity| identity.address.as_str());
        join_all(GEO_ENDPOINTS.iter().map(|endpoint| {
            probe_geo_endpoint(stack, AddressFamily::Ipv4, *endpoint, public_ip, cancel)
        }))
        .await
    };
    let ipv6 = async {
        if !selection.includes(AddressFamily::Ipv6) {
            return Vec::new();
        }
        let public_ip = identity
            .ipv6
            .as_ref()
            .map(|identity| identity.address.as_str());
        join_all(GEO_ENDPOINTS.iter().map(|endpoint| {
            probe_geo_endpoint(stack, AddressFamily::Ipv6, *endpoint, public_ip, cancel)
        }))
        .await
    };
    let (ipv4, ipv6) = join(ipv4, ipv6).await;

    GEO_ENDPOINTS
        .iter()
        .enumerate()
        .map(|(index, endpoint)| GeoResult {
            id: endpoint.id.to_string(),
            name: endpoint.name.to_string(),
            group: "geoip".to_string(),
            kind: "country".to_string(),
            ipv4: selection
                .includes(AddressFamily::Ipv4)
                .then(|| ipv4[index].clone()),
            ipv6: selection
                .includes(AddressFamily::Ipv6)
                .then(|| ipv6[index].clone()),
        })
        .collect()
}

async fn probe_geo_endpoint(
    stack: &NetworkStack,
    family: AddressFamily,
    endpoint: GeoEndpoint,
    public_ip: Option<&str>,
    cancel: &CancellationToken,
) -> GeoOutcome {
    let url = match (endpoint.parser, public_ip) {
        (GeoParser::IpWho, Some(address)) => format!("https://ipwho.is/{address}"),
        (GeoParser::CountryIs, Some(address)) => format!("https://api.country.is/{address}"),
        (GeoParser::IpApiCo, Some(address)) => format!("https://ipapi.co/{address}/json/"),
        _ => endpoint.url.to_string(),
    };
    let response = send_request(stack.client(family, false), Method::GET, &url, &[], cancel).await;
    match response {
        Ok(response) if matches!(response.status, 403 | 429) => GeoOutcome {
            state: "no_answer".to_string(),
            value: String::new(),
            country_name: String::new(),
            rtt_ms: response.rtt_ms,
            error: String::new(),
        },
        Ok(response) if !(200..300).contains(&response.status) => GeoOutcome {
            state: "error".to_string(),
            value: String::new(),
            country_name: String::new(),
            rtt_ms: response.rtt_ms,
            error: format!("HTTP {}", response.status),
        },
        Ok(response) => match parse_geo_response(endpoint.parser, &response.body) {
            Some((code, name)) => GeoOutcome {
                state: "success".to_string(),
                country_name: name.unwrap_or_else(|| country_name(&code)),
                value: code,
                rtt_ms: response.rtt_ms,
                error: String::new(),
            },
            None => GeoOutcome {
                state: "error".to_string(),
                value: String::new(),
                country_name: String::new(),
                rtt_ms: response.rtt_ms,
                error: "响应中没有有效国家代码".to_string(),
            },
        },
        Err(error) => GeoOutcome {
            state: if error.cancelled {
                "cancelled"
            } else {
                "error"
            }
            .to_string(),
            value: String::new(),
            country_name: String::new(),
            rtt_ms: 0.0,
            error: error.message,
        },
    }
}

fn parse_geo_response(parser: GeoParser, body: &str) -> Option<(String, Option<String>)> {
    if matches!(parser, GeoParser::CloudflareTrace) {
        let code = body.lines().find_map(|line| line.strip_prefix("loc="))?;
        return normalize_country_code(code).map(|code| (code, None));
    }

    let value: Value = serde_json::from_str(body).ok()?;
    if value.get("error").and_then(Value::as_bool) == Some(true)
        || value.get("success").and_then(Value::as_bool) == Some(false)
    {
        return None;
    }
    let (code, name) = match parser {
        GeoParser::IpWho => (
            value.get("country_code").and_then(Value::as_str),
            value.get("country").and_then(Value::as_str),
        ),
        GeoParser::CountryIs => (value.get("country").and_then(Value::as_str), None),
        GeoParser::IpApiCo => (
            value.get("country_code").and_then(Value::as_str),
            value.get("country_name").and_then(Value::as_str),
        ),
        GeoParser::CloudflareTrace => unreachable!(),
    };
    let code = normalize_country_code(code?)?;
    let name = name
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string);
    Some((code, name))
}

async fn run_portal(
    stack: &NetworkStack,
    family: AddressFamily,
    cancel: &CancellationToken,
) -> ConnectivityChecks {
    let results = join_all(
        PORTAL_ENDPOINTS
            .iter()
            .map(|endpoint| probe_portal_endpoint(stack, family, *endpoint, cancel)),
    )
    .await;
    summarize_portal(results)
}

async fn probe_portal_endpoint(
    stack: &NetworkStack,
    family: AddressFamily,
    endpoint: PortalEndpoint,
    cancel: &CancellationToken,
) -> PortalResult {
    match send_request(
        stack.client(family, true),
        Method::GET,
        endpoint.url,
        &[],
        cancel,
    )
    .await
    {
        Ok(response) => {
            let (verdict, detail) = classify_portal(
                endpoint,
                response.status,
                &response.body,
                &response.location,
            );
            PortalResult {
                id: endpoint.id.to_string(),
                name: endpoint.name.to_string(),
                vendor: endpoint.vendor.to_string(),
                url: endpoint.url.to_string(),
                verdict,
                status: response.status,
                rtt_ms: response.rtt_ms,
                detail,
                error: String::new(),
            }
        }
        Err(error) => PortalResult {
            id: endpoint.id.to_string(),
            name: endpoint.name.to_string(),
            vendor: endpoint.vendor.to_string(),
            url: endpoint.url.to_string(),
            verdict: if error.cancelled {
                "cancelled"
            } else {
                "unreachable"
            }
            .to_string(),
            status: 0,
            rtt_ms: 0.0,
            detail: String::new(),
            error: error.message,
        },
    }
}

fn classify_portal(
    endpoint: PortalEndpoint,
    status: u16,
    body: &str,
    location: &str,
) -> (String, String) {
    if (300..400).contains(&status) {
        let detail = if location.is_empty() {
            "端点返回重定向".to_string()
        } else {
            format!("重定向到 {location}")
        };
        return ("portal".to_string(), detail);
    }
    if status >= 500 {
        return (
            "unreachable".to_string(),
            "端点或网关返回服务器错误".to_string(),
        );
    }
    if status != endpoint.expected_status {
        return (
            "altered".to_string(),
            format!("预期 HTTP {}，实际 HTTP {status}", endpoint.expected_status),
        );
    }

    let body_matches = match endpoint.body {
        BodyExpectation::Empty => body.trim().is_empty(),
        BodyExpectation::Exact(expected) => body.trim() == expected.trim(),
    };
    if body_matches {
        ("clean".to_string(), "响应符合固定应答".to_string())
    } else {
        (
            "altered".to_string(),
            "响应正文与固定应答不一致".to_string(),
        )
    }
}

fn summarize_portal(results: Vec<PortalResult>) -> ConnectivityChecks {
    let has_clean = results.iter().any(|result| result.verdict == "clean");
    let has_plain_clean = results
        .iter()
        .any(|result| result.url.starts_with("http://") && result.verdict == "clean");
    let has_tls_clean = results
        .iter()
        .any(|result| result.url.starts_with("https://") && result.verdict == "clean");
    let plain_results = results
        .iter()
        .filter(|result| result.url.starts_with("http://"))
        .collect::<Vec<_>>();
    let plain_http_blocked = !plain_results.is_empty()
        && !has_plain_clean
        && has_tls_clean
        && plain_results
            .iter()
            .all(|result| result.verdict == "unreachable");
    let has_interference = results
        .iter()
        .any(|result| matches!(result.verdict.as_str(), "portal" | "altered"));
    ConnectivityChecks {
        clean: has_clean && has_plain_clean && !has_interference,
        plain_http_blocked,
        results,
    }
}

async fn send_request(
    client: &Client,
    method: Method,
    url: &str,
    headers: &[(&str, &str)],
    cancel: &CancellationToken,
) -> Result<HttpObservation, ProbeFailure> {
    if cancel.is_cancelled() {
        return Err(ProbeFailure::cancelled());
    }
    let started = Instant::now();
    let mut request = client.request(method, url);
    for (name, value) in headers {
        request = request.header(*name, *value);
    }
    let response = cancel
        .run_until_cancelled(request.send())
        .await
        .ok_or_else(ProbeFailure::cancelled)?
        .map_err(classify_request_error)?;
    let status = response.status().as_u16();
    let location = response
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    loop {
        let next = cancel
            .run_until_cancelled(stream.next())
            .await
            .ok_or_else(ProbeFailure::cancelled)?;
        let Some(chunk) = next else {
            break;
        };
        let chunk = chunk.map_err(classify_request_error)?;
        if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
            return Err(ProbeFailure::network("响应正文超过 4 MiB 限制"));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(HttpObservation {
        status,
        body: String::from_utf8_lossy(&body).into_owned(),
        location,
        rtt_ms: elapsed_ms_f64(started),
    })
}

fn classify_request_error(error: reqwest::Error) -> ProbeFailure {
    let message = if error.is_timeout() {
        "请求超时"
    } else if error.is_connect() {
        "连接失败"
    } else if error.is_redirect() {
        "重定向失败"
    } else if error.is_decode() {
        "响应解码失败"
    } else {
        "网络请求失败"
    };
    ProbeFailure::network(message)
}

async fn run_access(
    stack: &NetworkStack,
    family: AddressFamily,
    cancel: &CancellationToken,
) -> Vec<EndpointResult> {
    join_all(
        ACCESS_ENDPOINTS
            .iter()
            .map(|endpoint| probe_access_endpoint(stack, family, *endpoint, cancel)),
    )
    .await
}

async fn probe_access_endpoint(
    stack: &NetworkStack,
    family: AddressFamily,
    endpoint: AccessEndpoint,
    cancel: &CancellationToken,
) -> EndpointResult {
    let headers = [
        ("user-agent", BROWSER_USER_AGENT),
        ("accept", "text/html,application/xhtml+xml"),
        ("accept-language", "en-US,en;q=0.8"),
    ];
    match send_request(
        stack.client(family, false),
        Method::GET,
        endpoint.url,
        &headers,
        cancel,
    )
    .await
    {
        Ok(response) => {
            let (state, detail, region) =
                classify_access(endpoint.kind, response.status, &response.body);
            EndpointResult {
                id: endpoint.id.to_string(),
                name: endpoint.name.to_string(),
                vendor: Some(endpoint.vendor.to_string()),
                state,
                region,
                http_status: Some(response.status),
                detail,
                rtt_ms: response.rtt_ms,
                error: String::new(),
            }
        }
        Err(error) => endpoint_failure(endpoint.id, endpoint.name, Some(endpoint.vendor), error),
    }
}

fn classify_access(kind: AccessKind, status: u16, body: &str) -> (String, String, Option<String>) {
    let lower = body.to_ascii_lowercase();
    if is_challenge(status, &lower) {
        return (
            "error".to_string(),
            "检测到验证挑战，无法判断地区可用性".to_string(),
            None,
        );
    }

    if matches!(kind, AccessKind::ChatGpt) {
        return match chatgpt_unsupported_country(body) {
            Some(true) => (
                "blocked".to_string(),
                "ChatGPT 返回明确的地区限制信号".to_string(),
                None,
            ),
            Some(false) if (200..400).contains(&status) => (
                "available".to_string(),
                "ChatGPT 合规接口确认当前地区可用".to_string(),
                None,
            ),
            Some(false) => (
                "error".to_string(),
                format!("HTTP {status}，无法确认服务可用性"),
                None,
            ),
            None if contains_region_denial(&lower) => (
                "blocked".to_string(),
                "服务返回明确的地区限制信号".to_string(),
                None,
            ),
            None if (200..400).contains(&status) => (
                "available".to_string(),
                "官方页面可达，未发现明确地区拒绝".to_string(),
                None,
            ),
            None => (
                "error".to_string(),
                format!("HTTP {status}，无法确认服务可用性"),
                None,
            ),
        };
    }

    if contains_region_denial(&lower) {
        return (
            "blocked".to_string(),
            "服务返回明确的地区限制信号".to_string(),
            None,
        );
    }

    match kind {
        AccessKind::YoutubePremium => {
            if lower.contains("youtube premium")
                || lower.contains("ad-free")
                || lower.contains("ad free")
            {
                (
                    "available".to_string(),
                    "页面包含 Premium 服务信号".to_string(),
                    None,
                )
            } else {
                (
                    "error".to_string(),
                    "页面可达，但没有足够信号判断 Premium 可用性".to_string(),
                    None,
                )
            }
        }
        AccessKind::Gemini => match gemini_region(body) {
            Some(region)
                if matches!(
                    region.as_str(),
                    "RUS" | "BLR" | "CHN" | "PRK" | "IRN" | "CUB" | "SYR"
                ) =>
            {
                (
                    "blocked".to_string(),
                    "Gemini 页面返回受限地区标记".to_string(),
                    Some(region),
                )
            }
            Some(region) if (200..400).contains(&status) => (
                "available".to_string(),
                "Gemini 页面返回可识别的地区标记".to_string(),
                Some(region),
            ),
            _ => (
                "error".to_string(),
                "页面可达，但没有足够地区信号判断 Gemini 可用性".to_string(),
                None,
            ),
        },
        AccessKind::Claude if (200..400).contains(&status) => (
            "available".to_string(),
            "官方页面可达，未发现明确地区拒绝".to_string(),
            None,
        ),
        _ => (
            "error".to_string(),
            format!("HTTP {status}，无法确认服务可用性"),
            None,
        ),
    }
}

fn chatgpt_unsupported_country(body: &str) -> Option<bool> {
    serde_json::from_str::<Value>(body)
        .ok()?
        .get("unsupported_country")?
        .as_bool()
}

fn gemini_region(body: &str) -> Option<String> {
    static REGION_PATTERN: OnceLock<regex::Regex> = OnceLock::new();
    REGION_PATTERN
        .get_or_init(|| regex::Regex::new(r#",\d+,\d+,200,"([A-Z]{3})""#).unwrap())
        .captures(body)
        .and_then(|captures| captures.get(1))
        .map(|region| region.as_str().to_string())
}

async fn run_ai(
    stack: &NetworkStack,
    family: AddressFamily,
    cancel: &CancellationToken,
) -> Vec<EndpointResult> {
    join_all(
        AI_ENDPOINTS
            .iter()
            .map(|endpoint| probe_ai_endpoint(stack, family, *endpoint, cancel)),
    )
    .await
}

async fn probe_ai_endpoint(
    stack: &NetworkStack,
    family: AddressFamily,
    endpoint: AiEndpoint,
    cancel: &CancellationToken,
) -> EndpointResult {
    match send_request(
        stack.client(family, false),
        Method::GET,
        endpoint.url,
        endpoint.headers,
        cancel,
    )
    .await
    {
        Ok(response) => {
            let (state, detail) =
                classify_ai(response.status, &response.body, endpoint.auth_statuses);
            EndpointResult {
                id: endpoint.id.to_string(),
                name: endpoint.name.to_string(),
                vendor: Some(endpoint.vendor.to_string()),
                state,
                region: None,
                http_status: Some(response.status),
                detail,
                rtt_ms: response.rtt_ms,
                error: String::new(),
            }
        }
        Err(error) => endpoint_failure(endpoint.id, endpoint.name, Some(endpoint.vendor), error),
    }
}

fn classify_ai(status: u16, body: &str, auth_statuses: &[u16]) -> (String, String) {
    let lower = body.to_ascii_lowercase();
    if is_challenge(status, &lower) {
        return (
            "error".to_string(),
            "检测到验证挑战，尚未到达 API 认证层".to_string(),
        );
    }
    if contains_region_denial(&lower) {
        return (
            "blocked".to_string(),
            "API 返回明确的地区拒绝信号".to_string(),
        );
    }
    if auth_statuses.contains(&status) {
        return (
            "reachable".to_string(),
            "API 已响应认证或策略层；不代表账号或模型可用".to_string(),
        );
    }
    if (200..300).contains(&status) {
        return (
            "reachable".to_string(),
            "API 端点可达；未验证模型调用能力".to_string(),
        );
    }
    (
        "error".to_string(),
        format!("HTTP {status}，无法确认 API 可达性"),
    )
}

fn endpoint_failure(
    id: &str,
    name: &str,
    vendor: Option<&str>,
    error: ProbeFailure,
) -> EndpointResult {
    EndpointResult {
        id: id.to_string(),
        name: name.to_string(),
        vendor: vendor.map(str::to_string),
        state: if error.cancelled {
            "cancelled"
        } else {
            "error"
        }
        .to_string(),
        region: None,
        http_status: None,
        detail: String::new(),
        rtt_ms: 0.0,
        error: error.message,
    }
}

fn is_challenge(status: u16, lower_body: &str) -> bool {
    let has_marker = [
        "cf-chl-",
        "challenge-platform",
        "cf_chl_opt",
        "_cf_chl",
        "cf-turnstile",
        "cf-browser-verification",
        "checking your browser",
        "just a moment...",
    ]
    .iter()
    .any(|marker| lower_body.contains(marker));
    has_marker || (matches!(status, 403 | 429 | 503) && lower_body.contains("cloudflare ray id"))
}

fn contains_region_denial(lower_body: &str) -> bool {
    [
        "unsupported_country_region_territory",
        "unsupported_country",
        "country, region, or territory not supported",
        "request not allowed",
        "not available in your country",
        "not available in your region",
        "unavailable in your country",
        "country is not supported",
        "location is not supported",
        "service is not available in your country",
    ]
    .iter()
    .any(|marker| lower_body.contains(marker))
}

async fn run_path(family: AddressFamily, cancel: &CancellationToken) -> ConnectivityReport {
    let raw_targets = join_all(
        PATH_TARGETS
            .iter()
            .map(|target| probe_tcp_target(*target, family, cancel)),
    )
    .await;
    finalize_connectivity(raw_targets)
}

async fn probe_tcp_target(
    target: PathTarget,
    family: AddressFamily,
    cancel: &CancellationToken,
) -> RawPathTarget {
    let mut raw = RawPathTarget {
        target,
        resolved_ip: String::new(),
        samples: Vec::new(),
        attempts: 0,
        error: String::new(),
        cancelled: false,
    };
    if cancel.is_cancelled() {
        raw.cancelled = true;
        raw.error = "检测已取消".to_string();
        return raw;
    }

    let lookup = tokio::time::timeout(
        HTTP_CONNECT_TIMEOUT,
        lookup_host((target.host, target.port)),
    );
    let addresses = match cancel.run_until_cancelled(lookup).await {
        None => {
            raw.cancelled = true;
            raw.error = "检测已取消".to_string();
            return raw;
        }
        Some(result) => match result {
            Ok(Ok(addresses)) => addresses.collect::<Vec<_>>(),
            Ok(Err(_)) => {
                raw.error = "目标名称解析失败".to_string();
                return raw;
            }
            Err(_) => {
                raw.error = "目标名称解析超时".to_string();
                return raw;
            }
        },
    };
    let Some(address) = addresses
        .into_iter()
        .find(|address| family.matches(address.ip()))
    else {
        raw.error = format!("目标没有可用的 {} 地址", family.as_str());
        return raw;
    };
    raw.resolved_ip = address.ip().to_string();

    for _ in 0..TCP_ROUNDS {
        if cancel.is_cancelled() {
            raw.cancelled = true;
            raw.error = "检测已取消".to_string();
            break;
        }
        raw.attempts += 1;
        let started = Instant::now();
        let connect_result = connect_tcp(address, family, cancel).await;
        match connect_result {
            Ok(()) => raw.samples.push(elapsed_ms_f64(started)),
            Err(error) if error.cancelled => {
                raw.cancelled = true;
                raw.error = error.message;
                break;
            }
            Err(error) => raw.error = error.message,
        }
    }
    if !raw.cancelled && !raw.samples.is_empty() && raw.samples.len() < raw.attempts {
        raw.error = "部分 TCP 探测失败".to_string();
    }
    raw
}

async fn connect_tcp(
    address: SocketAddr,
    family: AddressFamily,
    cancel: &CancellationToken,
) -> Result<(), ProbeFailure> {
    let socket = match family {
        AddressFamily::Ipv4 => TcpSocket::new_v4(),
        AddressFamily::Ipv6 => TcpSocket::new_v6(),
    }
    .map_err(|_| ProbeFailure::network("无法创建 TCP socket"))?;
    socket
        .bind(SocketAddr::new(family.unspecified(), 0))
        .map_err(|_| ProbeFailure::network("无法绑定 TCP 地址族"))?;
    let connect = tokio::time::timeout(TCP_CONNECT_TIMEOUT, socket.connect(address));
    let stream = match cancel.run_until_cancelled(connect).await {
        None => return Err(ProbeFailure::cancelled()),
        Some(result) => match result {
            Ok(Ok(stream)) => stream,
            Ok(Err(_)) => return Err(ProbeFailure::network("TCP 连接失败")),
            Err(_) => return Err(ProbeFailure::network("TCP 连接超时")),
        },
    };
    drop(stream);
    Ok(())
}

fn finalize_connectivity(raw_targets: Vec<RawPathTarget>) -> ConnectivityReport {
    let mut target_rtts = raw_targets
        .iter()
        .filter(|target| !target.cancelled)
        .filter_map(|target| median(&target.samples))
        .collect::<Vec<_>>();
    target_rtts.sort_by(f64::total_cmp);
    let floor_ms = match target_rtts.len() {
        0 => 0.0,
        1..=3 => target_rtts[0],
        _ => target_rtts[1],
    };
    let median_rtt_ms = median(&target_rtts).unwrap_or(0.0);
    let targets = raw_targets
        .iter()
        .cloned()
        .map(|target| finalize_path_target(target, floor_ms))
        .collect::<Vec<_>>();
    let measured_scores = raw_targets
        .iter()
        .zip(targets.iter())
        .filter(|(raw, _)| !raw.cancelled && !raw.samples.is_empty())
        .map(|(_, target)| u32::from(target.verdict.score))
        .collect::<Vec<_>>();
    let score = if measured_scores.is_empty() {
        0
    } else {
        (measured_scores.iter().sum::<u32>() / measured_scores.len() as u32) as u8
    };
    ConnectivityReport {
        score,
        grade: score_grade(score).to_string(),
        floor_ms: round_ms(floor_ms),
        median_rtt_ms: round_ms(median_rtt_ms),
        capability: tcp_capability(),
        targets,
    }
}

fn finalize_path_target(target: RawPathTarget, floor_ms: f64) -> PathTargetResult {
    let rtt_ms = median(&target.samples).unwrap_or(0.0);
    let loss = if target.attempts == 0 {
        0.0
    } else {
        (target.attempts.saturating_sub(target.samples.len())) as f64 / target.attempts as f64
    };
    let jitter_ms = standard_deviation(&target.samples);
    let class = if target.cancelled && target.samples.is_empty() {
        "cancelled"
    } else if target.samples.is_empty() {
        "unreachable"
    } else {
        relative_path_class((rtt_ms - floor_ms).max(0.0))
    };
    let score = tcp_score(
        rtt_ms,
        floor_ms,
        jitter_ms,
        loss,
        !target.samples.is_empty(),
    );
    let mut notes = vec![
        "TCP 仅测量目标连接建立延迟".to_string(),
        "TCP 无法显示逐跳路径，也不能证明直连或 Transit".to_string(),
    ];
    if target.target.network.contains("Anycast") {
        notes.push("Anycast 延迟表示最近公告点，不表示服务器物理位置".to_string());
    }
    PathTargetResult {
        id: target.target.id.to_string(),
        name: target.target.name.to_string(),
        host: target.target.host.to_string(),
        network: target.target.network.to_string(),
        method: "tcp".to_string(),
        resolved_ip: target.resolved_ip,
        verdict: PathVerdict {
            class: class.to_string(),
            score,
            rtt_ms: round_ms(rtt_ms),
            jitter_ms: round_ms(jitter_ms),
            loss: round_ratio(loss),
            hop_count: 0,
            notes,
        },
        error: target.error,
    }
}

fn relative_path_class(excess_ms: f64) -> &'static str {
    if excess_ms <= 5.0 {
        "near_baseline"
    } else if excess_ms <= 20.0 {
        "local_region"
    } else if excess_ms <= 60.0 {
        "regional"
    } else {
        "distant"
    }
}

fn tcp_score(rtt_ms: f64, floor_ms: f64, jitter_ms: f64, loss: f64, reachable: bool) -> u8 {
    if !reachable {
        return 0;
    }
    let excess = (rtt_ms - floor_ms - 5.0).max(0.0);
    let absolute_latency = (rtt_ms.max(0.0) * 0.12).min(30.0);
    let penalty = (loss.clamp(0.0, 1.0) * 70.0)
        + (excess * 0.8).min(35.0)
        + (jitter_ms.max(0.0) * 1.5).min(15.0)
        + absolute_latency;
    (100.0 - penalty).clamp(0.0, 100.0).round() as u8
}

fn score_grade(score: u8) -> &'static str {
    match score {
        90..=100 => "A",
        75..=89 => "B",
        60..=74 => "C",
        40..=59 => "D",
        _ => "F",
    }
}

fn tcp_capability() -> PathCapability {
    PathCapability {
        icmp: false,
        raw: false,
        path_visible: false,
        hint: "当前桌面版使用 TCP 握手降级，只能测量目标连接延迟；看不到逐跳路径，不能据此证明直连或 Transit。".to_string(),
    }
}

fn build_findings(
    options: &RunPayload,
    identity: &IdentityReport,
    portal: &ConnectivityChecks,
    cancelled: bool,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    if !selected_identity_available(options.family, identity) {
        findings.push(Finding {
            id: "public_identity_unavailable".to_string(),
            title: "未检测到可用公网出口".to_string(),
            severity: "alert".to_string(),
            detail: "所有所选地址族的公网 IP 回显端点均未返回有效地址，无法完成网络质量诊断。"
                .to_string(),
        });
    }
    if options.family.includes(AddressFamily::Ipv4) && identity.ipv4.is_none() {
        findings.push(Finding {
            id: "ipv4_unavailable".to_string(),
            title: "未检测到可用 IPv4 出口".to_string(),
            severity: "warning".to_string(),
            detail: "所有 IPv4 公网地址回显端点均未返回有效地址。".to_string(),
        });
    }
    if options.family.includes(AddressFamily::Ipv6) && identity.ipv6.is_none() {
        findings.push(Finding {
            id: "ipv6_unavailable".to_string(),
            title: "未检测到可用 IPv6 出口".to_string(),
            severity: "warning".to_string(),
            detail: "当前网络可能没有全局 IPv6，或 IPv6 请求被阻断。".to_string(),
        });
    }
    for (family, label, result) in [
        (AddressFamily::Ipv4, "IPv4", identity.ipv4.as_ref()),
        (AddressFamily::Ipv6, "IPv6", identity.ipv6.as_ref()),
    ] {
        if !options.family.includes(family) {
            continue;
        }
        let Some(result) = result else {
            continue;
        };
        if result.country.is_none()
            && result.country_code.is_none()
            && result.region.is_none()
            && result.city.is_none()
        {
            findings.push(Finding {
                id: format!("{}_geolocation_unavailable", family.as_str()),
                title: format!("{label} 地理归属未识别"),
                severity: "warning".to_string(),
                detail:
                    "公网 IP 已识别，但可用归属来源均未返回国家或城市；可展开来源证据查看失败原因。"
                        .to_string(),
            });
        }
    }
    let portal_count = portal
        .results
        .iter()
        .filter(|result| result.verdict == "portal")
        .count();
    let altered_count = portal
        .results
        .iter()
        .filter(|result| result.verdict == "altered")
        .count();
    if portal_count > 0 {
        findings.push(Finding {
            id: "captive_portal".to_string(),
            title: "检测到网络门户重定向".to_string(),
            severity: "alert".to_string(),
            detail: format!("{portal_count} 个固定应答端点被重定向。"),
        });
    }
    if altered_count > 0 {
        findings.push(Finding {
            id: "portal_response_altered".to_string(),
            title: "固定连通性响应被改写".to_string(),
            severity: "warning".to_string(),
            detail: format!("{altered_count} 个端点的状态码或正文与预期不一致。"),
        });
    }
    if portal.plain_http_blocked {
        findings.push(Finding {
            id: "plain_http_blocked".to_string(),
            title: "明文 HTTP 可能被阻断".to_string(),
            severity: "warning".to_string(),
            detail: "HTTPS 固定应答成功，但全部明文 HTTP 固定应答不可达。".to_string(),
        });
    }
    if options.modules.path {
        findings.push(Finding {
            id: "tcp_path_limited".to_string(),
            title: "路径能力已降级为 TCP".to_string(),
            severity: "info".to_string(),
            detail: tcp_capability().hint,
        });
    }
    if cancelled {
        findings.push(Finding {
            id: "run_cancelled".to_string(),
            title: "检测已取消".to_string(),
            severity: "info".to_string(),
            detail: "报告保留取消前已经完成的结果。".to_string(),
        });
    }
    findings
}

fn parse_echo_ip(body: &str, family: AddressFamily) -> Option<IpAddr> {
    let cloudflare = body.lines().find_map(|line| {
        let (key, value) = line.trim().split_once('=')?;
        (key == "ip").then_some(value.trim())
    });
    let candidate = cloudflare.or_else(|| body.lines().find(|line| !line.trim().is_empty()))?;
    let mut address = candidate.trim().parse::<IpAddr>().ok()?;
    if let IpAddr::V6(ipv6) = address {
        if let Some(ipv4) = ipv6.to_ipv4_mapped() {
            address = IpAddr::V4(ipv4);
        }
    }
    (family.matches(address) && is_public_ip(address)).then_some(address)
}

fn select_public_ip(
    responses: &[Option<IpAddr>],
    family: AddressFamily,
) -> Option<(IpAddr, usize)> {
    let valid = responses
        .iter()
        .flatten()
        .copied()
        .filter(|address| family.matches(*address) && is_public_ip(*address))
        .collect::<Vec<_>>();
    let mut counts = HashMap::<IpAddr, usize>::new();
    for address in &valid {
        *counts.entry(*address).or_default() += 1;
    }
    let selected = valid
        .iter()
        .copied()
        .find(|address| counts.get(address).copied().unwrap_or_default() >= 2)
        .or_else(|| valid.first().copied())?;
    Some((selected, counts.get(&selected).copied().unwrap_or_default()))
}

fn is_public_ip(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let octets = address.octets();
            !address.is_unspecified()
                && !address.is_loopback()
                && !address.is_private()
                && !address.is_link_local()
                && !address.is_multicast()
                && address != Ipv4Addr::BROADCAST
                && !(octets[0] == 100 && (64..=127).contains(&octets[1]))
                && !(octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
                && !(octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
                && !(octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
                && !(octets[0] == 198 && matches!(octets[1], 18 | 19))
        }
        IpAddr::V6(address) => {
            let segments = address.segments();
            !address.is_unspecified()
                && !address.is_loopback()
                && !address.is_unique_local()
                && !address.is_unicast_link_local()
                && !address.is_multicast()
                && !(segments[0] == 0x2001 && segments[1] == 0x0db8)
        }
    }
}

fn normalize_country_code(value: &str) -> Option<String> {
    let value = value.trim();
    (value.len() == 2 && value.bytes().all(|byte| byte.is_ascii_alphabetic()))
        .then(|| value.to_ascii_uppercase())
}

fn country_consensus<'a, I>(codes: I) -> Vec<CountryConsensus>
where
    I: IntoIterator<Item = &'a str>,
{
    let valid = codes
        .into_iter()
        .filter_map(normalize_country_code)
        .collect::<Vec<_>>();
    let total = valid.len();
    let mut counts = HashMap::<String, usize>::new();
    for code in valid {
        *counts.entry(code).or_default() += 1;
    }
    let mut result = counts
        .into_iter()
        .map(|(code, count)| CountryConsensus {
            name: country_name(&code),
            code,
            count,
            total,
            percent: if total == 0 {
                0.0
            } else {
                ((count as f64 * 10000.0 / total as f64).round()) / 100.0
            },
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        right
            .count
            .cmp(&left.count)
            .then_with(|| left.code.cmp(&right.code))
    });
    result
}

fn country_name(code: &str) -> String {
    match code {
        "AU" => "Australia",
        "BR" => "Brazil",
        "CA" => "Canada",
        "CH" => "Switzerland",
        "CN" => "China",
        "DE" => "Germany",
        "ES" => "Spain",
        "FI" => "Finland",
        "FR" => "France",
        "GB" => "United Kingdom",
        "HK" => "Hong Kong",
        "ID" => "Indonesia",
        "IE" => "Ireland",
        "IN" => "India",
        "IT" => "Italy",
        "JP" => "Japan",
        "KR" => "South Korea",
        "MY" => "Malaysia",
        "NL" => "Netherlands",
        "NO" => "Norway",
        "NZ" => "New Zealand",
        "PH" => "Philippines",
        "PL" => "Poland",
        "RU" => "Russia",
        "SE" => "Sweden",
        "SG" => "Singapore",
        "TH" => "Thailand",
        "TW" => "Taiwan",
        "US" => "United States",
        "VN" => "Vietnam",
        _ => code,
    }
    .to_string()
}

fn json_integer(value: &Value) -> Option<u64> {
    value.as_u64().or_else(|| {
        value
            .as_str()?
            .trim()
            .trim_start_matches(|character: char| matches!(character, 'A' | 'a' | 'S' | 's'))
            .parse()
            .ok()
    })
}

fn first_non_empty_string(values: &[Option<&Value>]) -> Option<String> {
    values
        .iter()
        .filter_map(|value| value.and_then(Value::as_str))
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(str::to_string)
}

fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let middle = sorted.len() / 2;
    Some(if sorted.len() % 2 == 0 {
        (sorted[middle - 1] + sorted[middle]) / 2.0
    } else {
        sorted[middle]
    })
}

fn standard_deviation(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance = values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / (values.len() - 1) as f64;
    variance.sqrt()
}

fn elapsed_ms(started: Instant) -> u64 {
    started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64
}

fn elapsed_ms_f64(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.0
}

fn round_ms(value: f64) -> f64 {
    (value.max(0.0) * 100.0).round() / 100.0
}

fn round_ratio(value: f64) -> f64 {
    (value.clamp(0.0, 1.0) * 1000.0).round() / 1000.0
}

fn mask_report_identity(identity: &mut IdentityReport) {
    if let Some(ipv4) = &mut identity.ipv4 {
        ipv4.address = mask_ip(&ipv4.address);
    }
    if let Some(ipv6) = &mut identity.ipv6 {
        ipv6.address = mask_ip(&ipv6.address);
    }
}

fn mask_report_intelligence(report: &mut IpIntelligenceReport) {
    if let Some(family) = &mut report.ipv4 {
        mask_ip_family_intelligence(family);
    }
    if let Some(family) = &mut report.ipv6 {
        mask_ip_family_intelligence(family);
    }
}

fn mask_ip_family_intelligence(family: &mut IpFamilyIntelligence) {
    let original = family.ip.clone();
    let masked = mask_ip(&original);
    family.ip = masked.clone();
    for source in &mut family.sources {
        source.source_url = provider_homepage(&source.id).to_string();
        if let Some(error) = &mut source.error {
            *error = mask_text(error, &original, &masked);
        }
        if let Some(data) = &mut source.data {
            mask_ip_data(data, &original, &masked);
        }
    }
    mask_ip_consensus(&mut family.consensus, &original, &masked);
    for fact in &mut family.facts {
        for value in &mut fact.values {
            value.value = mask_text(&value.value, &original, &masked);
        }
    }
}

fn mask_ip_consensus(consensus: &mut IpConsensus, original: &str, masked: &str) {
    macro_rules! mask_field {
        ($field:ident) => {
            if let Some(value) = &mut consensus.$field {
                *value = mask_text(value, original, masked);
            }
        };
    }
    mask_field!(country_code);
    mask_field!(country_name);
    mask_field!(region);
    mask_field!(city);
    mask_field!(timezone);
    mask_field!(registered_country_code);
    mask_field!(asn);
    mask_field!(as_domain);
    mask_field!(isp);
    mask_field!(organization);
    mask_field!(announced_prefix);
    mask_field!(ptr);
    mask_field!(rir);
    mask_field!(allocation_cidr);
    mask_field!(allocation_date);
    mask_field!(abuse_contact);
    for value in &mut consensus.origin_asns {
        *value = mask_text(value, original, masked);
    }
    for value in &mut consensus.origin_holders {
        *value = mask_text(value, original, masked);
    }
}

fn mask_ip_data(data: &mut IpIntelligenceData, original: &str, masked: &str) {
    macro_rules! mask_field {
        ($field:ident) => {
            if let Some(value) = &mut data.$field {
                *value = mask_text(value, original, masked);
            }
        };
    }
    mask_field!(country_code);
    mask_field!(country_name);
    mask_field!(continent_code);
    mask_field!(continent_name);
    mask_field!(region);
    mask_field!(city);
    mask_field!(timezone);
    mask_field!(registered_country_code);
    mask_field!(asn);
    mask_field!(as_domain);
    mask_field!(isp);
    mask_field!(organization);
    mask_field!(announced_prefix);
    mask_field!(ptr);
    mask_field!(rir);
    mask_field!(allocation_cidr);
    mask_field!(allocation_date);
    mask_field!(abuse_contact);
    for value in &mut data.origin_asns {
        *value = mask_text(value, original, masked);
    }
    for value in &mut data.origin_holders {
        *value = mask_text(value, original, masked);
    }
}

fn mask_text(value: &str, original: &str, masked: &str) -> String {
    value.replace(original, masked)
}

fn provider_homepage(id: &str) -> &'static str {
    match id {
        "ipwho" => "https://ipwho.is/",
        "ipapi_is" => "https://ipapi.is/",
        "geojs" => "https://geojs.io/",
        "rdap" => "https://rdap.org/",
        "ripestat" => "https://stat.ripe.net/",
        _ => "",
    }
}

fn mask_ip(value: &str) -> String {
    match value.parse::<IpAddr>() {
        Ok(IpAddr::V4(address)) => {
            let octets = address.octets();
            format!("{}.{}.*.*", octets[0], octets[1])
        }
        Ok(IpAddr::V6(address)) => {
            let segments = address.segments();
            format!(
                "{:x}:{:x}:{:x}:{:x}:*:*:*:*",
                segments[0], segments[1], segments[2], segments[3]
            )
        }
        Err(_) => value.to_string(),
    }
}

fn demo_report(mask_ip_enabled: bool) -> RunReport {
    let mut identity = IdentityReport {
        ipv4: Some(IdentityFamily {
            address: "192.0.2.42".to_string(),
            asn: Some("AS64496".to_string()),
            isp: Some("Example Transit".to_string()),
            organization: Some("Example Network".to_string()),
            country: Some("United States".to_string()),
            country_code: Some("US".to_string()),
            region: Some("California".to_string()),
            city: Some("Los Angeles".to_string()),
            latitude: Some(34.0522),
            longitude: Some(-118.2437),
            timezone: Some("America/Los_Angeles".to_string()),
            source_votes: 4,
            attribution_source_count: 4,
        }),
        ipv6: Some(IdentityFamily {
            address: "2001:db8:1234:5678::42".to_string(),
            asn: Some("AS64496".to_string()),
            isp: Some("Example Transit".to_string()),
            organization: Some("Example Network".to_string()),
            country: Some("United States".to_string()),
            country_code: Some("US".to_string()),
            region: Some("California".to_string()),
            city: Some("Los Angeles".to_string()),
            latitude: Some(34.0522),
            longitude: Some(-118.2437),
            timezone: Some("America/Los_Angeles".to_string()),
            source_votes: 3,
            attribution_source_count: 4,
        }),
    };
    let mut ip_intelligence = IpIntelligenceReport {
        ipv4: Some(demo_ip_family_intelligence("192.0.2.42", "US")),
        ipv6: Some(demo_ip_family_intelligence("2001:db8:1234:5678::42", "US")),
    };
    let ipv4_codes = ["US", "US", "US", "CA"];
    let ipv6_codes = ["US", "US", "US", "US"];
    let geo = GEO_ENDPOINTS
        .iter()
        .enumerate()
        .map(|(index, endpoint)| GeoResult {
            id: endpoint.id.to_string(),
            name: endpoint.name.to_string(),
            group: "geoip".to_string(),
            kind: "country".to_string(),
            ipv4: Some(demo_geo_outcome(ipv4_codes[index], 12.0 + index as f64)),
            ipv6: Some(demo_geo_outcome(ipv6_codes[index], 14.0 + index as f64)),
        })
        .collect::<Vec<_>>();
    let portal_results = PORTAL_ENDPOINTS
        .iter()
        .enumerate()
        .map(|(index, endpoint)| PortalResult {
            id: endpoint.id.to_string(),
            name: endpoint.name.to_string(),
            vendor: endpoint.vendor.to_string(),
            url: endpoint.url.to_string(),
            verdict: "clean".to_string(),
            status: endpoint.expected_status,
            rtt_ms: 18.0 + index as f64,
            detail: "响应符合固定应答".to_string(),
            error: String::new(),
        })
        .collect::<Vec<_>>();
    let service_access = ACCESS_ENDPOINTS
        .iter()
        .enumerate()
        .map(|(index, endpoint)| EndpointResult {
            id: endpoint.id.to_string(),
            name: endpoint.name.to_string(),
            vendor: Some(endpoint.vendor.to_string()),
            state: "available".to_string(),
            region: (matches!(endpoint.kind, AccessKind::Gemini)).then(|| "USA".to_string()),
            http_status: Some(200),
            detail: "演示：官方页面可达".to_string(),
            rtt_ms: 24.0 + index as f64,
            error: String::new(),
        })
        .collect::<Vec<_>>();
    let ai_endpoints = AI_ENDPOINTS
        .iter()
        .enumerate()
        .map(|(index, endpoint)| EndpointResult {
            id: endpoint.id.to_string(),
            name: endpoint.name.to_string(),
            vendor: Some(endpoint.vendor.to_string()),
            state: "reachable".to_string(),
            region: None,
            http_status: Some(endpoint.auth_statuses[0]),
            detail: "演示：API 已响应认证层".to_string(),
            rtt_ms: 28.0 + index as f64,
            error: String::new(),
        })
        .collect::<Vec<_>>();
    let targets = PATH_TARGETS
        .iter()
        .enumerate()
        .map(|(index, target)| PathTargetResult {
            id: target.id.to_string(),
            name: target.name.to_string(),
            host: target.host.to_string(),
            network: target.network.to_string(),
            method: "tcp".to_string(),
            resolved_ip: format!("198.51.100.{}", index + 10),
            verdict: PathVerdict {
                class: if index < 2 {
                    "near_baseline"
                } else {
                    "local_region"
                }
                .to_string(),
                score: 96_u8.saturating_sub(index as u8 * 3),
                rtt_ms: 16.0 + index as f64 * 3.0,
                jitter_ms: 0.8 + index as f64 * 0.2,
                loss: 0.0,
                hop_count: 0,
                notes: vec![
                    "TCP 仅测量目标连接建立延迟".to_string(),
                    "TCP 无法显示逐跳路径，也不能证明直连或 Transit".to_string(),
                ],
            },
            error: String::new(),
        })
        .collect::<Vec<_>>();

    if mask_ip_enabled {
        mask_report_identity(&mut identity);
        mask_report_intelligence(&mut ip_intelligence);
    }
    RunReport {
        schema: 1,
        status: "success".to_string(),
        tool: TOOL_NAME.to_string(),
        timestamp: "2026-01-01T00:00:00Z".to_string(),
        duration_ms: 1250,
        transport: TransportReport {
            family: "auto".to_string(),
            mask_ip: mask_ip_enabled,
            resolver: "system".to_string(),
            proxy: "direct".to_string(),
        },
        identity,
        ip_intelligence,
        findings: vec![Finding {
            id: "tcp_path_limited".to_string(),
            title: "路径能力已降级为 TCP".to_string(),
            severity: "info".to_string(),
            detail: tcp_capability().hint,
        }],
        consensus: ConsensusReport {
            ipv4: country_consensus(ipv4_codes),
            ipv6: country_consensus(ipv6_codes),
        },
        geo,
        connectivity_checks: ConnectivityChecks {
            clean: true,
            plain_http_blocked: false,
            results: portal_results,
        },
        service_access,
        ai_endpoints,
        connectivity: ConnectivityReport {
            score: 90,
            grade: "A".to_string(),
            floor_ms: 16.0,
            median_rtt_ms: 22.0,
            capability: tcp_capability(),
            targets,
        },
    }
}

fn demo_ip_family_intelligence(ip: &str, country: &str) -> IpFamilyIntelligence {
    let geo = IpIntelligenceData {
        country_code: Some(country.to_string()),
        country_name: Some("United States".to_string()),
        continent_code: Some("NA".to_string()),
        continent_name: Some("North America".to_string()),
        region: Some("California".to_string()),
        city: Some("Los Angeles".to_string()),
        latitude: Some(34.0522),
        longitude: Some(-118.2437),
        timezone: Some("America/Los_Angeles".to_string()),
        asn: Some("AS64496".to_string()),
        as_domain: Some("example.net".to_string()),
        isp: Some("Example Transit".to_string()),
        organization: Some("Example Network".to_string()),
        ..IpIntelligenceData::default()
    };
    let registry = IpIntelligenceData {
        registered_country_code: Some(country.to_string()),
        rir: Some("ARIN".to_string()),
        allocation_cidr: Some("192.0.2.0/24".to_string()),
        allocation_date: Some("2020-01-01".to_string()),
        organization: Some("Example Network".to_string()),
        ..IpIntelligenceData::default()
    };
    let route = IpIntelligenceData {
        announced_prefix: Some("192.0.2.0/24".to_string()),
        ptr: Some("edge.example.net".to_string()),
        is_announced: Some(true),
        origin_asns: vec!["AS64496".to_string()],
        origin_holders: vec!["Example Network".to_string()],
        asn: Some("AS64496".to_string()),
        ..IpIntelligenceData::default()
    };
    let sources = vec![
        IpProviderResult {
            id: "ipwho".to_string(),
            name: "ipwho.is".to_string(),
            category: "geo".to_string(),
            source_url: format!("https://ipwho.is/{ip}"),
            status: "ok".to_string(),
            duration_ms: 42.0,
            data: Some(geo.clone()),
            error: None,
        },
        IpProviderResult {
            id: "geojs".to_string(),
            name: "GeoJS".to_string(),
            category: "network".to_string(),
            source_url: format!("https://get.geojs.io/v1/ip/geo/{ip}.json"),
            status: "ok".to_string(),
            duration_ms: 51.0,
            data: Some(geo),
            error: None,
        },
        IpProviderResult {
            id: "rdap".to_string(),
            name: "RDAP".to_string(),
            category: "registry".to_string(),
            source_url: format!("https://rdap.org/ip/{ip}"),
            status: "ok".to_string(),
            duration_ms: 68.0,
            data: Some(registry),
            error: None,
        },
        IpProviderResult {
            id: "ripestat".to_string(),
            name: "RIPEstat".to_string(),
            category: "network".to_string(),
            source_url: "https://stat.ripe.net/".to_string(),
            status: "ok".to_string(),
            duration_ms: 74.0,
            data: Some(route),
            error: None,
        },
    ];
    IpFamilyIntelligence {
        ip: ip.to_string(),
        consensus: build_ip_consensus(&sources),
        facts: build_ip_facts(&sources),
        sources,
    }
}

fn demo_geo_outcome(code: &str, rtt_ms: f64) -> GeoOutcome {
    GeoOutcome {
        state: "success".to_string(),
        value: code.to_string(),
        country_name: country_name(code),
        rtt_ms,
        error: String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn public_ip_vote_prefers_majority_then_catalog_first() {
        let cloudflare = "1.1.1.1".parse().unwrap();
        let google = "8.8.8.8".parse().unwrap();
        let quad9 = "9.9.9.9".parse().unwrap();
        let majority = select_public_ip(
            &[Some(cloudflare), Some(google), Some(cloudflare), None],
            AddressFamily::Ipv4,
        )
        .unwrap();
        assert_eq!(majority, (cloudflare, 2));

        let fallback = select_public_ip(
            &[Some(google), Some(quad9), Some(cloudflare)],
            AddressFamily::Ipv4,
        )
        .unwrap();
        assert_eq!(fallback, (google, 1));
    }

    #[test]
    fn echo_parser_handles_cloudflare_and_rejects_wrong_family() {
        let parsed = parse_echo_ip("fl=1\nip=1.1.1.1\nloc=US\n", AddressFamily::Ipv4);
        assert_eq!(parsed, Some("1.1.1.1".parse().unwrap()));
        assert_eq!(
            parse_echo_ip("2606:4700:4700::1111\n", AddressFamily::Ipv4),
            None
        );
        assert_eq!(parse_echo_ip("not-an-ip", AddressFamily::Ipv4), None);
    }

    #[test]
    fn country_normalization_and_consensus_are_stable() {
        assert_eq!(normalize_country_code(" us "), Some("US".to_string()));
        assert_eq!(normalize_country_code("USA"), None);
        assert_eq!(normalize_country_code("U1"), None);
        let result = country_consensus(["us", "DE", "US", "ca", ""]);
        assert_eq!(result[0].code, "US");
        assert_eq!(result[0].count, 2);
        assert_eq!(result[0].total, 4);
        assert_eq!(result[1].code, "CA");
        assert_eq!(result[2].code, "DE");
    }

    #[test]
    fn portal_classification_respects_redirect_and_fixed_body() {
        let endpoint = PORTAL_ENDPOINTS[0];
        assert_eq!(classify_portal(endpoint, 204, "", "").0, "clean");
        assert_eq!(
            classify_portal(endpoint, 204, "unexpected", "").0,
            "altered"
        );
        assert_eq!(
            classify_portal(endpoint, 302, "", "http://portal.local/").0,
            "portal"
        );
        assert_eq!(classify_portal(endpoint, 503, "", "").0, "unreachable");
    }

    #[test]
    fn portal_summary_detects_plain_http_blocking() {
        let mut results = PORTAL_ENDPOINTS
            .iter()
            .map(|endpoint| PortalResult {
                id: endpoint.id.to_string(),
                name: endpoint.name.to_string(),
                vendor: endpoint.vendor.to_string(),
                url: endpoint.url.to_string(),
                verdict: if endpoint.url.starts_with("https://") {
                    "clean"
                } else {
                    "unreachable"
                }
                .to_string(),
                status: 0,
                rtt_ms: 1.0,
                detail: String::new(),
                error: String::new(),
            })
            .collect::<Vec<_>>();
        let summary = summarize_portal(results.clone());
        assert!(summary.plain_http_blocked);
        assert!(!summary.clean);

        results[0].verdict = "clean".to_string();
        let summary = summarize_portal(results);
        assert!(!summary.plain_http_blocked);
        assert!(summary.clean);
    }

    #[test]
    fn ai_classifier_prioritizes_challenge_and_region_denial() {
        let challenge = classify_ai(403, "unsupported_country cf-chl-platform", AUTH_401_403);
        assert_eq!(challenge.0, "error");
        let blocked = classify_ai(401, "unsupported_country", AUTH_401);
        assert_eq!(blocked.0, "blocked");
        let reachable = classify_ai(401, "missing api key", AUTH_401);
        assert_eq!(reachable.0, "reachable");
        assert_eq!(classify_ai(418, "unknown", AUTH_401).0, "error");
    }

    #[test]
    fn chatgpt_compliance_uses_boolean_value_not_field_name() {
        let available =
            classify_access(AccessKind::ChatGpt, 200, r#"{"unsupported_country":false}"#);
        assert_eq!(available.0, "available");

        let blocked = classify_access(AccessKind::ChatGpt, 200, r#"{"unsupported_country":true}"#);
        assert_eq!(blocked.0, "blocked");

        let challenge = classify_access(
            AccessKind::ChatGpt,
            403,
            r#"{"unsupported_country":false,"message":"cf-chl-platform"}"#,
        );
        assert_eq!(challenge.0, "error");
    }

    #[test]
    fn ip_intelligence_parsers_and_facts_keep_provider_conflicts() {
        let ipwho = parse_ip_provider_data(
            IpProviderKind::IpWho,
            &json!({
                "success": true,
                "country_code": "US",
                "country": "United States",
                "region": "California",
                "city": "Los Angeles",
                "latitude": 34.05,
                "longitude": -118.24,
                "timezone": {"id": "America/Los_Angeles"},
                "connection": {
                    "asn": 64496,
                    "isp": "Example ISP",
                    "org": "Example Org",
                    "domain": "example.net"
                }
            }),
        )
        .unwrap();
        assert_eq!(ipwho.asn.as_deref(), Some("AS64496"));
        assert_eq!(ipwho.city.as_deref(), Some("Los Angeles"));
        assert_eq!(ipwho.latitude, Some(34.05));

        let ipapi_is = parse_ip_provider_data(
            IpProviderKind::IpApiIs,
            &json!({
                "ip": "192.0.2.42",
                "is_bogon": false,
                "company": "Example Network",
                "asn": "AS64496 Example Network",
                "city": "Los Angeles",
                "region": "California",
                "country": "United States",
                "lat": 34.05,
                "lon": -118.24,
                "timezone": "America/Los_Angeles"
            }),
        )
        .unwrap();
        assert_eq!(ipapi_is.country_name.as_deref(), Some("United States"));
        assert_eq!(ipapi_is.city.as_deref(), Some("Los Angeles"));
        assert_eq!(ipapi_is.asn.as_deref(), Some("AS64496"));
        assert_eq!(ipapi_is.organization.as_deref(), Some("Example Network"));

        let geojs = parse_ip_provider_data(
            IpProviderKind::GeoJs,
            &json!({
                "country_code": "CA",
                "country": "Canada",
                "region": "Ontario",
                "city": "Toronto",
                "asn": "64497",
                "organization": "Other Org"
            }),
        )
        .unwrap();
        let rdap = parse_ip_provider_data(
            IpProviderKind::Rdap,
            &json!({
                "country": "US",
                "name": "Example Allocation",
                "port43": "whois.arin.net",
                "cidr0_cidrs": [{"v4prefix": "192.0.2.0", "length": 24}],
                "events": [{"eventAction": "registration", "eventDate": "2020-01-01"}],
                "entities": [{
                    "roles": ["abuse"],
                    "vcardArray": ["vcard", [["email", {}, "text", "abuse@example.net"]]]
                }]
            }),
        )
        .unwrap();
        assert_eq!(rdap.rir.as_deref(), Some("ARIN"));
        assert_eq!(rdap.allocation_cidr.as_deref(), Some("192.0.2.0/24"));
        assert_eq!(rdap.abuse_contact.as_deref(), Some("abuse@example.net"));

        let sources = vec![
            IpProviderResult {
                id: "ipwho".to_string(),
                name: "ipwho.is".to_string(),
                category: "geo".to_string(),
                source_url: "https://ipwho.is/".to_string(),
                status: "ok".to_string(),
                duration_ms: 1.0,
                data: Some(ipwho),
                error: None,
            },
            IpProviderResult {
                id: "geojs".to_string(),
                name: "GeoJS".to_string(),
                category: "network".to_string(),
                source_url: "https://geojs.io/".to_string(),
                status: "ok".to_string(),
                duration_ms: 1.0,
                data: Some(geojs),
                error: None,
            },
            IpProviderResult {
                id: "rdap".to_string(),
                name: "RDAP".to_string(),
                category: "registry".to_string(),
                source_url: "https://rdap.org/".to_string(),
                status: "empty".to_string(),
                duration_ms: 1.0,
                data: Some(rdap),
                error: None,
            },
        ];
        let consensus = build_ip_consensus(&sources);
        assert_eq!(consensus.country_code.as_deref(), Some("US"));
        assert_eq!(consensus.source_count, 2);
        let country = build_ip_facts(&sources)
            .into_iter()
            .find(|fact| fact.key == "country")
            .unwrap();
        assert!(country.conflict);
        assert_eq!(country.source_count, 2);
    }

    #[test]
    fn geo_consensus_backfills_missing_identity_country() {
        let intelligence = IpFamilyIntelligence {
            ip: "192.0.2.42".to_string(),
            sources: Vec::new(),
            consensus: IpConsensus::default(),
            facts: Vec::new(),
        };
        let mut identity = IdentityReport {
            ipv4: Some(identity_from_intelligence(
                "192.0.2.42".parse().unwrap(),
                1,
                &intelligence,
            )),
            ipv6: None,
        };
        let consensus = ConsensusReport {
            ipv4: vec![CountryConsensus {
                code: "US".to_string(),
                name: "United States".to_string(),
                count: 3,
                total: 4,
                percent: 75.0,
            }],
            ipv6: Vec::new(),
        };

        backfill_identity_from_geo_consensus(&mut identity, &consensus);

        let ipv4 = identity.ipv4.unwrap();
        assert_eq!(ipv4.country_code.as_deref(), Some("US"));
        assert_eq!(ipv4.country.as_deref(), Some("United States"));
    }

    #[test]
    fn masked_demo_report_does_not_expose_original_addresses() {
        let report = serde_json::to_string(&demo_report(true)).unwrap();
        assert!(!report.contains("192.0.2.42"));
        assert!(!report.contains("2001:db8:1234:5678::42"));
        assert!(report.contains("192.0.*.*"));
        assert!(report.contains("2001:db8:1234:5678:*:*:*:*"));
        assert!(report.contains("ipIntelligence"));
        assert!(report.contains("https://ipwho.is/"));
    }

    #[test]
    fn export_masking_is_applied_to_unmasked_report_values() {
        let mut report = serde_json::to_value(demo_report(false)).unwrap();
        mask_export_value(&mut report);
        let text = serde_json::to_string(&report).unwrap();
        assert!(!text.contains("192.0.2.42"));
        assert!(!text.contains("2001:db8:1234:5678::42"));
        assert_eq!(report["transport"]["maskIp"], true);
        assert_eq!(
            report["ipIntelligence"]["ipv4"]["sources"][0]["sourceUrl"],
            "https://ipwho.is/"
        );
    }

    #[test]
    fn tcp_statistics_and_score_penalize_loss_and_distance() {
        assert_eq!(median(&[10.0, 30.0, 20.0]), Some(20.0));
        assert!((standard_deviation(&[10.0, 20.0, 30.0]) - 10.0).abs() < 0.01);
        assert_eq!(relative_path_class(4.9), "near_baseline");
        assert_eq!(relative_path_class(21.0), "regional");
        let clean = tcp_score(15.0, 12.0, 1.0, 0.0, true);
        let degraded = tcp_score(90.0, 12.0, 12.0, 1.0 / 3.0, true);
        assert!(clean > degraded);
        assert_eq!(tcp_score(0.0, 0.0, 0.0, 1.0, false), 0);
        assert!(tcp_score(20.0, 20.0, 0.0, 0.0, true) > tcp_score(200.0, 200.0, 0.0, 0.0, true));
    }

    #[test]
    fn connectivity_ignores_unmeasured_targets_and_uses_family_safe_quad9_host() {
        assert_eq!(PATH_TARGETS[4].host, "dns.quad9.net");
        let measured = RawPathTarget {
            target: PATH_TARGETS[0],
            resolved_ip: "1.1.1.1".to_string(),
            samples: vec![20.0, 22.0, 21.0],
            attempts: 3,
            error: String::new(),
            cancelled: false,
        };
        let dns_failure = RawPathTarget {
            target: PATH_TARGETS[1],
            resolved_ip: String::new(),
            samples: Vec::new(),
            attempts: 0,
            error: "目标名称解析失败".to_string(),
            cancelled: false,
        };
        let cancelled_partial = RawPathTarget {
            target: PATH_TARGETS[2],
            resolved_ip: "203.0.113.10".to_string(),
            samples: vec![250.0],
            attempts: 1,
            error: "检测已取消".to_string(),
            cancelled: true,
        };
        let report = finalize_connectivity(vec![measured, dns_failure, cancelled_partial]);
        assert_eq!(report.score, report.targets[0].verdict.score);
        assert_eq!(report.targets[1].verdict.loss, 0.0);
        assert_eq!(report.targets[1].verdict.score, 0);
        assert!(report.targets[2].verdict.score < report.score);
    }

    #[test]
    fn selected_identity_requires_a_public_address() {
        let empty = IdentityReport::default();
        assert!(!selected_identity_available(FamilySelection::Auto, &empty));
        assert!(!selected_identity_available(FamilySelection::Ipv4, &empty));
        assert!(!selected_identity_available(FamilySelection::Ipv6, &empty));
        let mut only_v6 = IdentityReport::default();
        only_v6.ipv6 = Some(IdentityFamily {
            address: "2001:db8::1".to_string(),
            asn: None,
            isp: None,
            organization: None,
            country: None,
            country_code: None,
            region: None,
            city: None,
            latitude: None,
            longitude: None,
            timezone: None,
            source_votes: 1,
            attribution_source_count: 0,
        });
        assert!(selected_identity_available(FamilySelection::Auto, &only_v6));
        assert!(!selected_identity_available(
            FamilySelection::Ipv4,
            &only_v6
        ));
        assert!(selected_identity_available(FamilySelection::Ipv6, &only_v6));
    }

    #[test]
    fn masks_ipv4_and_ipv6_addresses() {
        assert_eq!(mask_ip("203.0.113.42"), "203.0.*.*");
        assert_eq!(
            mask_ip("2001:db8:1234:5678::42"),
            "2001:db8:1234:5678:*:*:*:*"
        );
    }

    #[test]
    fn demo_report_is_deterministic_and_complete() {
        let first = serde_json::to_value(demo_report(false)).unwrap();
        let second = serde_json::to_value(demo_report(false)).unwrap();
        assert_eq!(first, second);
        assert_eq!(first["schema"], 1);
        assert_eq!(first["timestamp"], "2026-01-01T00:00:00Z");
        assert_eq!(first["geo"].as_array().unwrap().len(), 4);
        assert_eq!(
            first["connectivityChecks"]["results"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
        assert_eq!(first["serviceAccess"].as_array().unwrap().len(), 4);
        assert_eq!(first["aiEndpoints"].as_array().unwrap().len(), 8);
        assert_eq!(
            first["connectivity"]["targets"].as_array().unwrap().len(),
            5
        );
        assert_eq!(first["connectivity"]["capability"]["pathVisible"], false);
    }

    #[test]
    fn no_redirect_client_preserves_portal_response() {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(async {
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let address = listener.local_addr().unwrap();
                let server = tokio::spawn(async move {
                    let (mut stream, _) = listener.accept().await.unwrap();
                    let mut request = [0_u8; 1024];
                    let _ = stream.read(&mut request).await.unwrap();
                    stream
                        .write_all(
                            b"HTTP/1.1 302 Found\r\nLocation: http://portal.local/\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        )
                        .await
                        .unwrap();
                });
                let client = build_http_client(AddressFamily::Ipv4, true).unwrap();
                let cancel = CancellationToken::new();
                let response = send_request(
                    &client,
                    Method::GET,
                    &format!("http://{address}/"),
                    &[],
                    &cancel,
                )
                .await
                .unwrap();
                server.await.unwrap();
                assert_eq!(response.status, 302);
                assert_eq!(response.location, "http://portal.local/");
            });
    }
}

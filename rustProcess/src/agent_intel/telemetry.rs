use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};

use agent_intel::AgentIntelSnapshot;
use agent_intel::domain::{
    AgentAttention, AgentAttentionKind, AgentExceptionalKind, AgentExceptionalState,
    AgentLifecycle, AgentSource, AgentSourceKind, CurrentAgentActivity,
};
use base64::Engine as _;
use kodosi_domain::ids::SessionId;
use subtle::ConstantTimeEq;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc, oneshot};

const MAX_HEADER_BYTES: usize = 16 * 1024;
const MAX_BODY_BYTES: usize = 1024 * 1024;
const EVENT_CAPACITY: usize = 256;

static COLLECTOR: OnceLock<Collector> = OnceLock::new();

struct Collector {
    base_url: String,
    events: broadcast::Sender<Observation>,
    extension_events: broadcast::Sender<ExtensionObservation>,
    active_registrations: Arc<Mutex<HashMap<String, ActiveRegistration>>>,
    permission_bridges: Arc<Mutex<HashMap<IncarnationKey, PermissionBridge>>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct IncarnationKey {
    session_id: String,
    local_incarnation_id: uuid::Uuid,
}

#[derive(Clone)]
struct ActiveRegistration {
    key: IncarnationKey,
    token: String,
}

pub(crate) struct SessionRegistration {
    key: IncarnationKey,
    token: String,
    base_url: String,
    active_registrations: Arc<Mutex<HashMap<String, ActiveRegistration>>>,
}

impl SessionRegistration {
    pub(crate) fn local_incarnation_id(&self) -> uuid::Uuid {
        self.key.local_incarnation_id
    }

    pub(crate) fn claude_telemetry_environment(&self) -> Vec<(String, String)> {
        vec![
            ("CLAUDE_CODE_ENABLE_TELEMETRY".to_owned(), "1".to_owned()),
            ("OTEL_LOGS_EXPORTER".to_owned(), "otlp".to_owned()),
            (
                "OTEL_EXPORTER_OTLP_LOGS_PROTOCOL".to_owned(),
                "http/json".to_owned(),
            ),
            (
                "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT".to_owned(),
                self.endpoint("logs"),
            ),
            ("OTEL_LOGS_EXPORT_INTERVAL".to_owned(), "1000".to_owned()),
            ("OTEL_LOG_USER_PROMPTS".to_owned(), "0".to_owned()),
            ("OTEL_LOG_ASSISTANT_RESPONSES".to_owned(), "0".to_owned()),
            ("OTEL_LOG_TOOL_DETAILS".to_owned(), "0".to_owned()),
        ]
    }

    pub(crate) fn extension_endpoint(&self) -> String {
        self.endpoint("copilot")
    }

    pub(crate) fn extension_permission_endpoint(&self) -> String {
        self.endpoint("copilot-permission")
    }

    fn endpoint(&self, route: &str) -> String {
        format!(
            "{}/v1/{route}/{}/{}/{}",
            self.base_url, self.key.session_id, self.key.local_incarnation_id, self.token
        )
    }
}

impl Drop for SessionRegistration {
    fn drop(&mut self) {
        if let Ok(mut registrations) = self.active_registrations.lock()
            && registrations
                .get(&self.key.session_id)
                .is_some_and(|active| capability_matches(active, &self.key, &self.token))
        {
            registrations.remove(&self.key.session_id);
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Observation {
    session_id: String,
    local_incarnation_id: uuid::Uuid,
    event_name: String,
    timestamp: String,
    attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExtensionObservation {
    pub(crate) session_id: String,
    pub(crate) event_type: String,
    pub(crate) timestamp: String,
    pub(crate) data: serde_json::Value,
}

pub(crate) struct ObservedBoundary {
    pub(crate) kind: ObservedBoundaryKind,
    pub(crate) tool_use_id: Option<String>,
}

#[derive(Clone, Copy)]
pub(crate) enum ObservedBoundaryKind {
    Tool,
    Turn,
}

pub(crate) struct CopilotPermissionRequest {
    pub(crate) local_incarnation_id: uuid::Uuid,
    pub(crate) request_id: String,
    pub(crate) permission_request: serde_json::Value,
    pub(crate) reply: oneshot::Sender<CopilotPermissionDecision>,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub(crate) enum CopilotPermissionDecision {
    ApproveOnce,
    Reject { feedback: String },
    NoResult,
}

#[derive(Clone)]
struct PermissionBridge {
    generation: uuid::Uuid,
    sender: mpsc::Sender<CopilotPermissionRequest>,
}

pub(crate) struct PermissionSubscription {
    key: IncarnationKey,
    generation: uuid::Uuid,
    receiver: mpsc::Receiver<CopilotPermissionRequest>,
    bridges: Arc<Mutex<HashMap<IncarnationKey, PermissionBridge>>>,
}

impl PermissionSubscription {
    pub(crate) async fn recv(&mut self) -> Option<CopilotPermissionRequest> {
        self.receiver.recv().await
    }
}

impl Drop for PermissionSubscription {
    fn drop(&mut self) {
        if let Ok(mut bridges) = self.bridges.lock()
            && bridges
                .get(&self.key)
                .is_some_and(|bridge| bridge.generation == self.generation)
        {
            bridges.remove(&self.key);
        }
    }
}

impl Observation {
    pub(crate) fn belongs_to(
        &self,
        session_id: SessionId,
        local_incarnation_id: uuid::Uuid,
    ) -> bool {
        self.session_id == session_id.to_string()
            && self.local_incarnation_id == local_incarnation_id
    }

    pub(crate) fn boundary(&self) -> Option<ObservedBoundary> {
        let name = self
            .event_name
            .strip_prefix("claude_code.")
            .unwrap_or(&self.event_name);
        match name {
            "tool_result" => Some(ObservedBoundary {
                kind: ObservedBoundaryKind::Tool,
                tool_use_id: self.attributes.get("tool_use_id").cloned(),
            }),
            "api_request"
                if self.attributes.get("stop_reason").is_some_and(|reason| {
                    matches!(reason.as_str(), "end_turn" | "stop_sequence" | "refusal")
                }) =>
            {
                Some(ObservedBoundary {
                    kind: ObservedBoundaryKind::Turn,
                    tool_use_id: None,
                })
            }
            _ => None,
        }
    }

    pub(crate) fn opens_turn(&self) -> bool {
        let name = self
            .event_name
            .strip_prefix("claude_code.")
            .unwrap_or(&self.event_name);
        matches!(name, "user_prompt" | "api_request")
    }
}

impl ExtensionObservation {
    pub(crate) fn belongs_to(
        &self,
        session_id: SessionId,
        local_incarnation_id: uuid::Uuid,
    ) -> bool {
        self.session_id == session_id.to_string()
            && self
                .data
                .get("localIncarnationId")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| uuid::Uuid::parse_str(value).ok())
                == Some(local_incarnation_id)
    }

    pub(crate) fn boundary(&self) -> Option<ObservedBoundary> {
        match self.event_type.as_str() {
            "tool.execution_complete" => Some(ObservedBoundary {
                kind: ObservedBoundaryKind::Tool,
                tool_use_id: self
                    .data
                    .get("toolCallId")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
            }),
            "session.idle" => Some(ObservedBoundary {
                kind: ObservedBoundaryKind::Turn,
                tool_use_id: None,
            }),
            _ => None,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ClaudeTelemetryState {
    current_activity: Option<CurrentAgentActivity>,
    exceptional_state: Option<AgentExceptionalState>,
    attention: Option<AgentAttention>,
    model: Option<String>,
    vendor_session_id: Option<String>,
    seen: bool,
}

impl ClaudeTelemetryState {
    pub(crate) fn apply(&mut self, observation: &Observation) {
        self.seen = true;
        if let Some(vendor_session_id) = observation.attributes.get("session.id") {
            self.vendor_session_id = Some(vendor_session_id.clone());
        }
        if let Some(model) = observation.attributes.get("model") {
            self.model = Some(model.clone());
        }

        let name = observation
            .event_name
            .strip_prefix("claude_code.")
            .unwrap_or(&observation.event_name);
        let activity = match name {
            "user_prompt" => Some("Processing a user request".to_owned()),
            "assistant_response" => Some("Produced an assistant response".to_owned()),
            "api_request" => Some("Calling the model".to_owned()),
            "tool_result" => {
                let tool = observation
                    .attributes
                    .get("tool_name")
                    .map_or("tool", String::as_str);
                Some(format!("Completed {tool}"))
            }
            "tool_decision" => Some("Resolved a tool permission request".to_owned()),
            "api_error" => Some("A model request failed".to_owned()),
            _ => None,
        };
        if let Some(summary) = activity {
            self.current_activity = Some(CurrentAgentActivity {
                summary,
                last_progress_at: Some(observation.timestamp.clone()),
            });
        }

        match name {
            "api_error" => {
                let summary = observation
                    .attributes
                    .get("error")
                    .cloned()
                    .unwrap_or_else(|| "Claude reported a model request failure".to_owned());
                let status = observation
                    .attributes
                    .get("status_code")
                    .map(String::as_str);
                let lower = summary.to_ascii_lowercase();
                let kind = if matches!(status, Some("401" | "403")) {
                    AgentExceptionalKind::Authentication
                } else if status == Some("429") || lower.contains("rate limit") {
                    AgentExceptionalKind::RateLimit
                } else if lower.contains("quota") {
                    AgentExceptionalKind::Quota
                } else if lower.contains("context") && lower.contains("limit") {
                    AgentExceptionalKind::ContextOverflow
                } else {
                    AgentExceptionalKind::Other
                };
                let summary = bounded(&summary, 240);
                self.exceptional_state = Some(AgentExceptionalState {
                    kind,
                    summary: summary.clone(),
                    retryable: !matches!(kind, AgentExceptionalKind::Quota),
                });
                self.attention = Some(AgentAttention {
                    kind: match kind {
                        AgentExceptionalKind::Authentication => AgentAttentionKind::Authentication,
                        AgentExceptionalKind::RateLimit
                        | AgentExceptionalKind::Quota
                        | AgentExceptionalKind::ContextOverflow => AgentAttentionKind::Limit,
                        _ => AgentAttentionKind::Failure,
                    },
                    summary,
                    actionable: true,
                });
            }
            "tool_result"
                if observation.attributes.get("success").map(String::as_str) == Some("false") =>
            {
                let tool = observation
                    .attributes
                    .get("tool_name")
                    .map_or("Tool", String::as_str);
                let summary = format!("{tool} failed");
                self.exceptional_state = Some(AgentExceptionalState {
                    kind: AgentExceptionalKind::Other,
                    summary: summary.clone(),
                    retryable: true,
                });
                self.attention = Some(AgentAttention {
                    kind: AgentAttentionKind::Failure,
                    summary,
                    actionable: true,
                });
            }
            "user_prompt" | "assistant_response" | "api_request" | "tool_result" => {
                self.exceptional_state = None;
                self.attention = None;
            }
            _ => {}
        }
    }

    pub(crate) fn overlay(&self, snapshot: &mut AgentIntelSnapshot) {
        if !self.seen {
            return;
        }
        if let Some(model) = &self.model {
            snapshot.identity.model = Some(bounded(model, 240));
        }
        if let Some(vendor_session_id) = &self.vendor_session_id {
            snapshot.identity.vendor_session_id = Some(bounded(vendor_session_id, 240));
        }
        if let Some(activity) = &self.current_activity {
            snapshot.current_activity = Some(activity.clone());
        }
        if let Some(exceptional_state) = &self.exceptional_state {
            snapshot.exceptional_state = Some(exceptional_state.clone());
            snapshot.attention.clone_from(&self.attention);
            snapshot.lifecycle = AgentLifecycle::Waiting;
        }
        snapshot.source = AgentSource {
            kind: AgentSourceKind::Telemetry,
            degraded: false,
            detail: Some(
                "Identity and lifecycle from `claude agents --json`; progress from per-session OpenTelemetry"
                    .to_owned(),
            ),
        };
    }
}

pub(crate) const CLAUDE_TELEMETRY_CONFLICTING_ENV: &[&str] = &[
    "CLAUDE_CODE_ENABLE_TELEMETRY",
    "OTEL_LOGS_EXPORTER",
    "OTEL_EXPORTER_OTLP_ENDPOINT",
    "OTEL_EXPORTER_OTLP_PROTOCOL",
    "OTEL_EXPORTER_OTLP_HEADERS",
    "OTEL_EXPORTER_OTLP_LOGS_ENDPOINT",
    "OTEL_EXPORTER_OTLP_LOGS_PROTOCOL",
    "OTEL_EXPORTER_OTLP_LOGS_HEADERS",
    "OTEL_LOGS_EXPORT_INTERVAL",
    "OTEL_LOG_USER_PROMPTS",
    "OTEL_LOG_ASSISTANT_RESPONSES",
    "OTEL_LOG_TOOL_DETAILS",
];

pub(crate) fn register_session(
    session_id: SessionId,
    local_incarnation_id: uuid::Uuid,
) -> Result<SessionRegistration, String> {
    let collector = ensure_collector()?;
    let mut token_bytes = [0_u8; 32];
    getrandom::fill(&mut token_bytes)
        .map_err(|error| format!("could not create local session capability: {error}"))?;
    let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token_bytes);
    let key = IncarnationKey {
        session_id: session_id.to_string(),
        local_incarnation_id,
    };
    let active = ActiveRegistration {
        key: key.clone(),
        token: token.clone(),
    };
    collector
        .active_registrations
        .lock()
        .map_err(|_| "local session capability registry is unavailable".to_owned())?
        .insert(key.session_id.clone(), active);
    Ok(SessionRegistration {
        key,
        token,
        base_url: collector.base_url.clone(),
        active_registrations: Arc::clone(&collector.active_registrations),
    })
}

pub(crate) fn subscribe() -> Option<broadcast::Receiver<Observation>> {
    COLLECTOR
        .get()
        .map(|collector| collector.events.subscribe())
}

pub(crate) fn subscribe_extensions() -> Option<broadcast::Receiver<ExtensionObservation>> {
    COLLECTOR
        .get()
        .map(|collector| collector.extension_events.subscribe())
}

pub(crate) fn register_permission_bridge(
    session_id: SessionId,
    local_incarnation_id: uuid::Uuid,
) -> Option<PermissionSubscription> {
    let collector = COLLECTOR.get()?;
    let generation = uuid::Uuid::now_v7();
    let (sender, receiver) = mpsc::channel(8);
    let key = IncarnationKey {
        session_id: session_id.to_string(),
        local_incarnation_id,
    };
    let is_active = collector
        .active_registrations
        .lock()
        .ok()?
        .get(&key.session_id)
        .is_some_and(|active| active.key == key);
    if !is_active {
        return None;
    }
    let mut bridges = collector.permission_bridges.lock().ok()?;
    bridges.insert(key.clone(), PermissionBridge { generation, sender });
    drop(bridges);
    Some(PermissionSubscription {
        key,
        generation,
        receiver,
        bridges: Arc::clone(&collector.permission_bridges),
    })
}

fn ensure_collector() -> Result<&'static Collector, String> {
    if let Some(collector) = COLLECTOR.get() {
        return Ok(collector);
    }
    let std_listener = std::net::TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| format!("could not bind Claude telemetry receiver: {error}"))?;
    std_listener
        .set_nonblocking(true)
        .map_err(|error| format!("could not configure Claude telemetry receiver: {error}"))?;
    let address = std_listener
        .local_addr()
        .map_err(|error| format!("could not inspect Claude telemetry receiver: {error}"))?;
    let listener = tokio::net::TcpListener::from_std(std_listener)
        .map_err(|error| format!("could not start Claude telemetry receiver: {error}"))?;
    let (events, _) = broadcast::channel(EVENT_CAPACITY);
    let (extension_events, _) = broadcast::channel(EVENT_CAPACITY);
    let active_registrations = Arc::new(Mutex::new(HashMap::new()));
    let permission_bridges = Arc::new(Mutex::new(HashMap::new()));
    let collector = Collector {
        base_url: format!("http://{address}"),
        events: events.clone(),
        extension_events: extension_events.clone(),
        active_registrations: Arc::clone(&active_registrations),
        permission_bridges: Arc::clone(&permission_bridges),
    };
    if COLLECTOR.set(collector).is_err() {
        return COLLECTOR
            .get()
            .ok_or_else(|| "Claude telemetry receiver initialization raced".to_owned());
    }
    tokio::spawn(serve(
        listener,
        events,
        extension_events,
        active_registrations,
        permission_bridges,
    ));
    COLLECTOR
        .get()
        .ok_or_else(|| "Claude telemetry receiver was not retained".to_owned())
}

async fn serve(
    listener: tokio::net::TcpListener,
    events: broadcast::Sender<Observation>,
    extension_events: broadcast::Sender<ExtensionObservation>,
    active_registrations: Arc<Mutex<HashMap<String, ActiveRegistration>>>,
    permission_bridges: Arc<Mutex<HashMap<IncarnationKey, PermissionBridge>>>,
) {
    loop {
        let Ok((stream, _peer)) = listener.accept().await else {
            break;
        };
        let events = events.clone();
        let extension_events = extension_events.clone();
        let active_registrations = Arc::clone(&active_registrations);
        let permission_bridges = Arc::clone(&permission_bridges);
        tokio::spawn(async move {
            if let Err(error) = handle_connection(
                stream,
                &events,
                &extension_events,
                &active_registrations,
                &permission_bridges,
            )
            .await
            {
                tracing::debug!(%error, "ignored malformed local agent event request");
            }
        });
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "one bounded request handler preserves authentication-before-routing order"
)]
async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    events: &broadcast::Sender<Observation>,
    extension_events: &broadcast::Sender<ExtensionObservation>,
    active_registrations: &Mutex<HashMap<String, ActiveRegistration>>,
    permission_bridges: &Mutex<HashMap<IncarnationKey, PermissionBridge>>,
) -> Result<(), String> {
    let mut request = Vec::with_capacity(4096);
    let header_end = loop {
        if request.len() >= MAX_HEADER_BYTES {
            return Err("telemetry request headers exceeded the limit".to_owned());
        }
        let mut chunk = [0_u8; 4096];
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|error| format!("could not read telemetry request: {error}"))?;
        if read == 0 {
            return Err("telemetry request ended before its headers".to_owned());
        }
        request.extend_from_slice(&chunk[..read]);
        if let Some(index) = request.windows(4).position(|window| window == b"\r\n\r\n") {
            break index + 4;
        }
    };
    let (route, content_length) = {
        let headers = std::str::from_utf8(&request[..header_end])
            .map_err(|error| format!("telemetry headers are not UTF-8: {error}"))?;
        let mut lines = headers.split("\r\n");
        let request_line = lines
            .next()
            .ok_or_else(|| "telemetry request line is missing".to_owned())?;
        let mut request_parts = request_line.split_whitespace();
        if request_parts.next() != Some("POST") {
            return Err("telemetry receiver accepts only POST".to_owned());
        }
        let path = request_parts
            .next()
            .ok_or_else(|| "telemetry request path is missing".to_owned())?;
        let route = parse_request_route(path)?;
        let content_length = lines
            .filter_map(|line| line.split_once(':'))
            .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
            .map(|(_, value)| value.trim())
            .ok_or_else(|| "telemetry request has no Content-Length".to_owned())?
            .parse::<usize>()
            .map_err(|error| format!("telemetry Content-Length is invalid: {error}"))?;
        (route, content_length)
    };
    if content_length > MAX_BODY_BYTES {
        return Err("telemetry request body exceeded the limit".to_owned());
    }
    while request.len() - header_end < content_length {
        let remaining = content_length - (request.len() - header_end);
        let mut chunk = vec![0_u8; remaining.min(4096)];
        let read = stream
            .read(&mut chunk)
            .await
            .map_err(|error| format!("could not read telemetry body: {error}"))?;
        if read == 0 {
            return Err("telemetry request body ended early".to_owned());
        }
        request.extend_from_slice(&chunk[..read]);
    }

    let body = &request[header_end..header_end + content_length];
    let response = match route {
        RequestRoute::Claude(claim) => {
            let key = authorize_claim(&claim, active_registrations)?;
            for observation in parse_observations(&key, body)? {
                drop(events.send(observation));
            }
            b"{}".to_vec()
        }
        RequestRoute::Copilot(claim) => {
            let key = authorize_claim(&claim, active_registrations)?;
            let wire: ExtensionEventWire = serde_json::from_slice(body)
                .map_err(|error| format!("Copilot extension event is invalid JSON: {error}"))?;
            if !supported_extension_event(&wire.event_type) {
                return Err("Copilot extension event type is unsupported".to_owned());
            }
            let mut data = redact_extension_data(&wire.event_type, &wire.data);
            if let Some(data) = data.as_object_mut() {
                data.insert(
                    "localIncarnationId".to_owned(),
                    serde_json::Value::String(key.local_incarnation_id.to_string()),
                );
            }
            drop(extension_events.send(ExtensionObservation {
                session_id: key.session_id,
                event_type: wire.event_type,
                timestamp: now_rfc3339(),
                data,
            }));
            b"{}".to_vec()
        }
        RequestRoute::CopilotPermission(claim) => {
            let key = authorize_claim(&claim, active_registrations)?;
            let wire: ExtensionPermissionWire = serde_json::from_slice(body)
                .map_err(|error| format!("Copilot permission request is invalid JSON: {error}"))?;
            let sender = permission_bridges
                .lock()
                .ok()
                .and_then(|bridges| bridges.get(&key).map(|bridge| bridge.sender.clone()));
            let decision = if let Some(sender) = sender {
                let (reply, decision) = oneshot::channel();
                let request = CopilotPermissionRequest {
                    local_incarnation_id: key.local_incarnation_id,
                    request_id: wire.request_id,
                    permission_request: wire.permission_request,
                    reply,
                };
                match tokio::time::timeout(std::time::Duration::from_secs(1), sender.send(request))
                    .await
                {
                    Ok(Ok(())) => {
                        tokio::time::timeout(std::time::Duration::from_mins(10), decision)
                            .await
                            .ok()
                            .and_then(Result::ok)
                            .unwrap_or(CopilotPermissionDecision::NoResult)
                    }
                    Ok(Err(_)) | Err(_) => CopilotPermissionDecision::NoResult,
                }
            } else {
                CopilotPermissionDecision::NoResult
            };
            serde_json::to_vec(&decision)
                .map_err(|error| format!("could not encode Copilot permission decision: {error}"))?
        }
    };
    let headers = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        response.len()
    );
    stream
        .write_all(headers.as_bytes())
        .await
        .map_err(|error| format!("could not reply to local agent event: {error}"))?;
    stream
        .write_all(&response)
        .await
        .map_err(|error| format!("could not reply to local agent event: {error}"))
}

#[derive(Debug, PartialEq, Eq)]
enum RequestRoute {
    Claude(CapabilityClaim),
    Copilot(CapabilityClaim),
    CopilotPermission(CapabilityClaim),
}

#[derive(Debug, PartialEq, Eq)]
struct CapabilityClaim {
    key: IncarnationKey,
    token: String,
}

#[derive(serde::Deserialize)]
struct ExtensionEventWire {
    #[serde(rename = "type")]
    event_type: String,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExtensionPermissionWire {
    request_id: String,
    permission_request: serde_json::Value,
}

#[expect(
    clippy::option_if_let_else,
    reason = "explicit ordered route matching is clearer for security-sensitive capability paths"
)]
fn parse_request_route(path: &str) -> Result<RequestRoute, String> {
    if let Some(path) = path.strip_prefix("/v1/logs/") {
        parse_capability_claim(path).map(RequestRoute::Claude)
    } else if let Some(path) = path.strip_prefix("/v1/copilot/") {
        parse_capability_claim(path).map(RequestRoute::Copilot)
    } else if let Some(path) = path.strip_prefix("/v1/copilot-permission/") {
        parse_capability_claim(path).map(RequestRoute::CopilotPermission)
    } else {
        Err("local agent event request path is invalid".to_owned())
    }
}

fn parse_capability_claim(path: &str) -> Result<CapabilityClaim, String> {
    let mut segments = path.split('/');
    let session_id = segments.next().unwrap_or_default();
    let incarnation_id = segments.next().unwrap_or_default();
    let token = segments.next().unwrap_or_default();
    if segments.next().is_some()
        || session_id.is_empty()
        || session_id.len() > 64
        || !session_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || token.len() != 43
        || !token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err("local agent event capability path is invalid".to_owned());
    }
    let local_incarnation_id = uuid::Uuid::parse_str(incarnation_id)
        .map_err(|_| "local agent event capability path is invalid".to_owned())?;
    Ok(CapabilityClaim {
        key: IncarnationKey {
            session_id: session_id.to_owned(),
            local_incarnation_id,
        },
        token: token.to_owned(),
    })
}

fn authorize_claim(
    claim: &CapabilityClaim,
    active_registrations: &Mutex<HashMap<String, ActiveRegistration>>,
) -> Result<IncarnationKey, String> {
    let registrations = active_registrations
        .lock()
        .map_err(|_| "local session capability registry is unavailable".to_owned())?;
    registrations
        .get(&claim.key.session_id)
        .filter(|active| capability_matches(active, &claim.key, &claim.token))
        .map(|active| active.key.clone())
        .ok_or_else(|| "local agent event capability is inactive".to_owned())
}

fn capability_matches(active: &ActiveRegistration, key: &IncarnationKey, token: &str) -> bool {
    active.key == *key
        && active.token.len() == token.len()
        && bool::from(active.token.as_bytes().ct_eq(token.as_bytes()))
}

fn redact_extension_data(event_type: &str, data: &serde_json::Value) -> serde_json::Value {
    match event_type {
        "tool.execution_start" => serde_json::json!({
            "toolCategory": tool_category(data),
            "toolCallId": bounded_identifier(data, "toolCallId"),
        }),
        "tool.execution_complete" => {
            let success = data.get("success").and_then(serde_json::Value::as_bool) == Some(true);
            serde_json::json!({
                "toolCategory": tool_category(data),
                "toolCallId": bounded_identifier(data, "toolCallId"),
                "success": success,
                "errorCategory": if success {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String("tool-execution-failed".to_owned())
                },
            })
        }
        "permission.requested" => serde_json::json!({
            "kind": permission_category(data),
        }),
        "session.idle" => serde_json::json!({
            "aborted": data.get("aborted").and_then(serde_json::Value::as_bool) == Some(true),
        }),
        "session.error" => serde_json::json!({
            "errorCategory": session_error_category(data),
        }),
        "session.shutdown" => serde_json::json!({
            "shutdownCategory": shutdown_category(data),
        }),
        "user.message" => serde_json::json!({
            "sourceCategory": source_category(data),
        }),
        "kodosi.room.offered" | "kodosi.room.accepted" | "kodosi.room.acted" => {
            room_state_data(data, false)
        }
        "kodosi.room.delivery_failed" => room_state_data(data, true),
        _ => serde_json::json!({}),
    }
}

fn bounded_identifier(data: &serde_json::Value, field: &str) -> Option<String> {
    data.get(field)
        .and_then(serde_json::Value::as_str)
        .filter(|value| !value.is_empty())
        .map(|value| value.chars().take(240).collect())
}

fn supported_extension_event(event_type: &str) -> bool {
    matches!(
        event_type,
        "assistant.turn_start"
            | "assistant.message"
            | "tool.execution_start"
            | "tool.execution_complete"
            | "permission.requested"
            | "session.idle"
            | "session.error"
            | "session.shutdown"
            | "user.message"
            | "kodosi.extension.ready"
            | "kodosi.channel.ready"
            | "kodosi.room.offered"
            | "kodosi.room.accepted"
            | "kodosi.room.acted"
            | "kodosi.room.delivery_failed"
    )
}

fn tool_category(data: &serde_json::Value) -> &'static str {
    let value = data
        .get("toolCategory")
        .or_else(|| data.get("toolName"))
        .and_then(serde_json::Value::as_str);
    match value {
        Some(value)
            if ["shell", "bash", "powershell"]
                .iter()
                .any(|candidate| value.eq_ignore_ascii_case(candidate)) =>
        {
            "shell"
        }
        Some(value)
            if ["read", "view"]
                .iter()
                .any(|candidate| value.eq_ignore_ascii_case(candidate)) =>
        {
            "read"
        }
        Some(value)
            if ["write", "edit", "create"]
                .iter()
                .any(|candidate| value.eq_ignore_ascii_case(candidate)) =>
        {
            "write"
        }
        Some(value)
            if ["search", "grep", "glob"]
                .iter()
                .any(|candidate| value.eq_ignore_ascii_case(candidate)) =>
        {
            "search"
        }
        Some(value) if value.eq_ignore_ascii_case("task") => "task",
        Some(value)
            if ["web", "webfetch", "websearch"]
                .iter()
                .any(|candidate| value.eq_ignore_ascii_case(candidate)) =>
        {
            "web"
        }
        Some(value)
            if ["mcp", "kodosi_room_reply", "kodosi_room_task_transition"]
                .iter()
                .any(|candidate| value.eq_ignore_ascii_case(candidate)) =>
        {
            "integration"
        }
        _ => "other",
    }
}

fn permission_category(data: &serde_json::Value) -> &'static str {
    match data.get("kind").and_then(serde_json::Value::as_str) {
        Some("shell") => "shell",
        Some("write") => "write",
        Some("read") => "read",
        Some("mcp") => "mcp",
        Some("url") => "url",
        _ => "other",
    }
}

fn session_error_category(data: &serde_json::Value) -> &'static str {
    let value = data
        .get("errorCategory")
        .or_else(|| data.get("errorType"))
        .and_then(serde_json::Value::as_str);
    match value {
        Some("authentication") => "authentication",
        Some("rate-limit") => "rate-limit",
        Some("quota") => "quota",
        Some("context-limit") => "context-limit",
        Some("network") => "network",
        _ => "other",
    }
}

fn shutdown_category(data: &serde_json::Value) -> &'static str {
    let value = data
        .get("shutdownCategory")
        .or_else(|| data.get("shutdownType"))
        .and_then(serde_json::Value::as_str);
    match value {
        Some("completed") => "completed",
        Some("cancelled") => "cancelled",
        Some("error") => "error",
        _ => "other",
    }
}

fn source_category(data: &serde_json::Value) -> &'static str {
    let value = data
        .get("sourceCategory")
        .or_else(|| data.get("source"))
        .and_then(serde_json::Value::as_str);
    match value {
        Some("user") => "user",
        Some("system") => "system",
        Some("extension") => "extension",
        _ => "other",
    }
}

fn room_state_data(data: &serde_json::Value, failed: bool) -> serde_json::Value {
    let mut safe = serde_json::Map::new();
    if let Some(event_id) = data
        .get("eventId")
        .and_then(serde_json::Value::as_str)
        .filter(|value| {
            !value.is_empty()
                && value.len() <= 240
                && value.bytes().all(|byte| {
                    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b':' | b'.')
                })
        })
    {
        safe.insert(
            "eventId".to_owned(),
            serde_json::Value::String(event_id.to_owned()),
        );
    }
    if failed {
        safe.insert(
            "errorCategory".to_owned(),
            serde_json::Value::String("room-delivery-failed".to_owned()),
        );
        safe.insert(
            "message".to_owned(),
            serde_json::Value::String("Room event delivery failed".to_owned()),
        );
    }
    serde_json::Value::Object(safe)
}

fn parse_observations(key: &IncarnationKey, bytes: &[u8]) -> Result<Vec<Observation>, String> {
    let value: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|error| format!("telemetry body is not OTLP JSON: {error}"))?;
    let mut records = Vec::new();
    collect_log_records(&value, &mut records);
    Ok(records
        .into_iter()
        .filter_map(|record| observation_from_record(key, record))
        .collect())
}

fn collect_log_records<'a>(value: &'a serde_json::Value, records: &mut Vec<&'a serde_json::Value>) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object {
                if key == "logRecords" {
                    if let Some(items) = value.as_array() {
                        records.extend(items);
                    }
                } else {
                    collect_log_records(value, records);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_log_records(item, records);
            }
        }
        _ => {}
    }
}

fn observation_from_record(
    key: &IncarnationKey,
    record: &serde_json::Value,
) -> Option<Observation> {
    let attributes = record
        .get("attributes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|attribute| {
            let key = attribute.get("key")?.as_str()?.to_owned();
            let value = any_value(attribute.get("value")?)?;
            Some((key, value))
        })
        .collect::<BTreeMap<_, _>>();
    let body = record.get("body").and_then(any_value);
    let event_name = attributes
        .get("event.name")
        .cloned()
        .or(body)
        .filter(|name| !name.is_empty())?;
    let timestamp = attributes
        .get("event.timestamp")
        .cloned()
        .unwrap_or_else(now_rfc3339);
    Some(Observation {
        session_id: key.session_id.clone(),
        local_incarnation_id: key.local_incarnation_id,
        event_name,
        timestamp,
        attributes,
    })
}

fn any_value(value: &serde_json::Value) -> Option<String> {
    let object = value.as_object()?;
    for key in ["stringValue", "intValue", "doubleValue", "boolValue"] {
        if let Some(value) = object.get(key) {
            return value
                .as_str()
                .map(str::to_owned)
                .or_else(|| value.as_bool().map(|value| value.to_string()))
                .or_else(|| value.as_f64().map(|value| value.to_string()));
        }
    }
    None
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

fn bounded(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_otlp_json_without_content_fields() {
        let session_id = SessionId::new();
        let key = IncarnationKey {
            session_id: session_id.to_string(),
            local_incarnation_id: uuid::Uuid::now_v7(),
        };
        let body = serde_json::json!({
            "resourceLogs": [{
                "scopeLogs": [{
                    "logRecords": [{
                        "body": { "stringValue": "claude_code.tool_result" },
                        "attributes": [
                            { "key": "event.name", "value": { "stringValue": "tool_result" } },
                            { "key": "event.timestamp", "value": { "stringValue": "2026-08-23T12:00:00Z" } },
                            { "key": "tool_name", "value": { "stringValue": "Bash" } },
                            { "key": "tool_use_id", "value": { "stringValue": "tool-1" } },
                            { "key": "success", "value": { "boolValue": false } },
                            { "key": "session.id", "value": { "stringValue": "native-session" } }
                        ]
                    }]
                }]
            }]
        });
        let observations =
            parse_observations(&key, &serde_json::to_vec(&body).expect("json")).expect("parse");
        assert_eq!(observations.len(), 1);
        assert_eq!(observations[0].session_id, session_id.to_string());
        assert_eq!(
            observations[0].local_incarnation_id,
            key.local_incarnation_id
        );
        assert_eq!(observations[0].event_name, "tool_result");
        let boundary = observations[0].boundary().expect("tool boundary");
        assert!(matches!(boundary.kind, ObservedBoundaryKind::Tool));
        assert_eq!(boundary.tool_use_id.as_deref(), Some("tool-1"));
        assert!(observations[0].belongs_to(session_id, key.local_incarnation_id));
        assert!(!observations[0].belongs_to(session_id, uuid::Uuid::now_v7()));

        let mut state = ClaudeTelemetryState::default();
        state.apply(&observations[0]);
        let mut snapshot = AgentIntelSnapshot::default();
        state.overlay(&mut snapshot);
        assert_eq!(snapshot.source.kind, AgentSourceKind::Telemetry);
        assert_eq!(snapshot.lifecycle, AgentLifecycle::Waiting);
        assert_eq!(
            snapshot.attention.expect("attention").kind,
            AgentAttentionKind::Failure
        );
        assert_eq!(
            snapshot.identity.vendor_session_id.as_deref(),
            Some("native-session")
        );
    }

    #[test]
    fn rejects_non_otlp_json() {
        let key = IncarnationKey {
            session_id: SessionId::new().to_string(),
            local_incarnation_id: uuid::Uuid::now_v7(),
        };
        assert!(parse_observations(&key, b"not-json").is_err());
    }

    #[test]
    fn claude_end_turn_telemetry_is_a_turn_boundary() {
        let observation = Observation {
            session_id: SessionId::new().to_string(),
            local_incarnation_id: uuid::Uuid::now_v7(),
            event_name: "api_request".to_owned(),
            timestamp: "now".to_owned(),
            attributes: BTreeMap::from([("stop_reason".to_owned(), "end_turn".to_owned())]),
        };
        assert!(observation.opens_turn());
        assert!(matches!(
            observation.boundary().map(|boundary| boundary.kind),
            Some(ObservedBoundaryKind::Turn)
        ));
    }

    #[test]
    fn stale_capability_is_rejected_and_cannot_unregister_current_incarnation() {
        let registrations = Arc::new(Mutex::new(HashMap::new()));
        let session_id = SessionId::new().to_string();
        let old_key = IncarnationKey {
            session_id: session_id.clone(),
            local_incarnation_id: uuid::Uuid::now_v7(),
        };
        let current_key = IncarnationKey {
            session_id: session_id.clone(),
            local_incarnation_id: uuid::Uuid::now_v7(),
        };
        let old_token = "a".repeat(43);
        let current_token = "b".repeat(43);
        registrations.lock().expect("registry").insert(
            session_id.clone(),
            ActiveRegistration {
                key: old_key.clone(),
                token: old_token.clone(),
            },
        );
        let old_guard = SessionRegistration {
            key: old_key.clone(),
            token: old_token.clone(),
            base_url: "http://127.0.0.1:1".to_owned(),
            active_registrations: Arc::clone(&registrations),
        };
        let old_claims = ["logs", "copilot", "copilot-permission"].map(|route| {
            let route = parse_request_route(&format!(
                "/v1/{route}/{}/{}/{}",
                old_key.session_id, old_key.local_incarnation_id, old_token
            ))
            .expect("old route");
            match route {
                RequestRoute::Claude(claim)
                | RequestRoute::Copilot(claim)
                | RequestRoute::CopilotPermission(claim) => claim,
            }
        });
        for claim in &old_claims {
            assert_eq!(
                authorize_claim(claim, &registrations).expect("old active"),
                old_key
            );
        }

        registrations.lock().expect("registry").insert(
            session_id,
            ActiveRegistration {
                key: current_key.clone(),
                token: current_token.clone(),
            },
        );
        let current_guard = SessionRegistration {
            key: current_key.clone(),
            token: current_token.clone(),
            base_url: "http://127.0.0.1:1".to_owned(),
            active_registrations: Arc::clone(&registrations),
        };
        assert!(
            old_claims
                .iter()
                .all(|claim| authorize_claim(claim, &registrations).is_err())
        );
        drop(old_guard);
        let current_claim = parse_capability_claim(&format!(
            "{}/{}/{}",
            current_key.session_id, current_key.local_incarnation_id, current_token
        ))
        .expect("current claim");
        assert_eq!(
            authorize_claim(&current_claim, &registrations).expect("current remains active"),
            current_key
        );
        drop(current_guard);
        assert!(authorize_claim(&current_claim, &registrations).is_err());
    }

    #[test]
    fn extension_data_redacts_vendor_error_content() {
        let secret = "/Users/alice/private prompt: rm -rf project";
        let tool = redact_extension_data(
            "tool.execution_complete",
            &serde_json::json!({
                "toolName": secret,
                "success": false,
                "error": secret,
                "arguments": secret,
                "result": secret,
                "stderr": secret,
            }),
        );
        assert_eq!(tool["toolCategory"], "other");
        assert_eq!(tool["errorCategory"], "tool-execution-failed");
        assert!(!tool.to_string().contains(secret));

        let session = redact_extension_data(
            "session.error",
            &serde_json::json!({"errorType":"unknown-secret", "message":secret}),
        );
        assert_eq!(session, serde_json::json!({"errorCategory":"other"}));
        assert!(!session.to_string().contains(secret));
    }

    #[test]
    fn extension_observations_require_exact_incarnation() {
        let session_id = SessionId::new();
        let local_incarnation_id = uuid::Uuid::now_v7();
        let observation = ExtensionObservation {
            session_id: session_id.to_string(),
            event_type: "session.idle".to_owned(),
            timestamp: "2026-08-23T12:00:00Z".to_owned(),
            data: serde_json::json!({
                "localIncarnationId": local_incarnation_id.to_string(),
                "aborted": false,
            }),
        };
        assert!(observation.belongs_to(session_id, local_incarnation_id));
        assert!(!observation.belongs_to(session_id, uuid::Uuid::now_v7()));
        assert!(!observation.belongs_to(SessionId::new(), local_incarnation_id));
        assert!(matches!(
            observation.boundary().map(|boundary| boundary.kind),
            Some(ObservedBoundaryKind::Turn)
        ));

        let tool = ExtensionObservation {
            session_id: session_id.to_string(),
            event_type: "tool.execution_complete".to_owned(),
            timestamp: "2026-08-23T12:00:01Z".to_owned(),
            data: serde_json::json!({
                "localIncarnationId": local_incarnation_id.to_string(),
                "toolCallId": "tool-2",
                "success": true,
            }),
        };
        let boundary = tool.boundary().expect("tool boundary");
        assert!(matches!(boundary.kind, ObservedBoundaryKind::Tool));
        assert_eq!(boundary.tool_use_id.as_deref(), Some("tool-2"));
    }
}

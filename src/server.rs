use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, Request, State};
use axum::http::header::{
    AUTHORIZATION, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_SECURITY_POLICY, CONTENT_TYPE, HOST,
    ORIGIN, REFERRER_POLICY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD as BASE64_STANDARD, URL_SAFE_NO_PAD};
use futures_util::stream;
use futures_util::{SinkExt as _, StreamExt as _};
use include_dir::{Dir, include_dir};
use serde::Serialize;
use serde_json::{Map, Value, json};
use sha2::{Digest as _, Sha256};
use time::OffsetDateTime;
use tokio::net::TcpListener;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore, broadcast};
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

use crate::computer::{COMPUTER_HELPER_ORIGIN, COMPUTER_METHODS, COMPUTER_SHARE_ACK_CAPABILITY};
use crate::error_taxonomy::classify;
use crate::hub::{ExtensionHub, HubError};
use crate::token::{create_token, token_is_valid, tokens_equal};
use crate::update::{UpdateState, UpdateStatus, check_for_update};
use crate::ws_auth::{
    AUTH_TIMEOUT, BROWSER_CONNECTOR, COMPUTER_CONNECTOR, MAX_AUTH_MESSAGE_BYTES, MAX_AUTH_MESSAGES,
    MAX_PROVISIONAL_CONNECTIONS, ServerChallenge,
};
use crate::{PROTOCOL_VERSION, VERSION};

const MAX_BODY_BYTES: usize = 128 * 1024;
const MAX_ACTIVITY: usize = 80;
const MAX_SCREENSHOT_BYTES: usize = 8 * 1024 * 1024;
const MAX_WS_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const CALL_ID_MAX_CHARS: usize = 128;
const REPLAY_CACHE_ENTRIES: usize = 256;
const REPLAY_CACHE_TTL: Duration = Duration::from_secs(10 * 60);
const NORMALIZED_COORDINATE_MAX: f64 = 1_000.0;
/// Cross-origin frame bounds. `MAX_FRAME_INDEX` mirrors the extension's
/// 16-frame attachment cap, so a `f<k>` the extension can never mint is
/// rejected before relay.
const MAX_FRAME_INDEX: u32 = 16;
const MAX_OBSERVED_FRAMES: usize = 16;
const MAX_OBSERVED_FRAME_SKIPS: usize = 32;
const MAX_OBSERVED_FRAME_DEPTH: u64 = 5;
/// Publication cap for one observation's element list, and the share of it
/// reserved for elements that came from a merged cross-origin frame.
const MAX_PUBLISHED_ELEMENTS: usize = 250;
const MAX_PUBLISHED_FRAME_ELEMENTS: usize = 50;
const FRAME_SUMMARY_COUNTER_MAX: u64 = 10_000;
const PUBLIC_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/public");

pub const ACTION_METHODS: &[&str] = &[
    "status",
    "browser.control.start",
    "browser.control.status",
    "browser.control.stop",
    "tabs.list",
    "tabs.activate",
    "tabs.new",
    "tabs.close",
    "page.observe",
    "page.navigate",
    "page.back",
    "page.forward",
    "page.reload",
    "page.click",
    "page.fill",
    "page.select",
    "page.key",
    "page.scroll",
    "page.clickAt",
    "page.typeText",
    "page.evaluate",
    "page.waitFor",
    "page.hover",
    "page.batch",
    "page.handleDialog",
];

/// The only methods a `page.batch` sub-action may use: snapshot-bound page
/// interactions with no navigation, no evaluation, and no nested batching.
const BATCH_SUB_METHODS: &[&str] = &[
    "page.click",
    "page.fill",
    "page.select",
    "page.key",
    "page.scroll",
];
const BATCH_MAX_ACTIONS: usize = 10;

#[derive(Clone)]
pub struct ServerConfig {
    pub port: u16,
    pub token: String,
    pub call_timeout: Duration,
    pub check_for_updates: bool,
}

impl ServerConfig {
    pub fn new(port: u16, token: impl Into<String>) -> Self {
        Self {
            port,
            token: token.into(),
            call_timeout: Duration::from_secs(15),
            check_for_updates: true,
        }
    }
}

pub struct BridgeServer {
    listener: TcpListener,
    router: Router,
    state: AppState,
}

impl BridgeServer {
    pub async fn bind(config: ServerConfig) -> Result<Self, std::io::Error> {
        if !token_is_valid(&config.token) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Bridge token must be a canonical URL-safe encoding of 32 random bytes",
            ));
        }
        let listener = TcpListener::bind(("127.0.0.1", config.port)).await?;
        let bound_port = listener.local_addr()?.port();
        let state = AppState::new(
            config.token,
            bound_port,
            config.call_timeout,
            config.check_for_updates,
        );
        let router = build_router(state.clone());
        if config.check_for_updates {
            let update_state = state.clone();
            tokio::spawn(async move {
                update_state.refresh_update().await;
            });
        }
        Ok(Self {
            listener,
            router,
            state,
        })
    }

    pub fn local_addr(&self) -> Result<SocketAddr, std::io::Error> {
        self.listener.local_addr()
    }

    pub async fn serve<F>(self, shutdown: F) -> Result<(), std::io::Error>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let result = axum::serve(self.listener, self.router)
            .with_graceful_shutdown(shutdown)
            .await;
        self.state.hub.close();
        self.state.computer_hub.close();
        result
    }
}

#[derive(Clone)]
struct AppState {
    token: Arc<String>,
    bound_port: u16,
    hub: ExtensionHub,
    computer_hub: ExtensionHub,
    data: Arc<RwLock<StateData>>,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    command_replay: Arc<Mutex<CommandReplay>>,
    events: broadcast::Sender<ServerEvent>,
    action_lock: Arc<tokio::sync::Mutex<()>>,
    update_lock: Arc<tokio::sync::Mutex<()>>,
    browser_auth_slots: Arc<Semaphore>,
    computer_auth_slots: Arc<Semaphore>,
}

impl AppState {
    fn new(
        token: String,
        bound_port: u16,
        call_timeout: Duration,
        check_for_updates: bool,
    ) -> Self {
        let (events, _) = broadcast::channel(256);
        let mut data = StateData::default();
        data.public.update = if check_for_updates {
            UpdateStatus::checking()
        } else {
            UpdateStatus::disabled()
        };
        Self {
            token: Arc::new(token),
            bound_port,
            hub: ExtensionHub::new(call_timeout),
            computer_hub: ExtensionHub::computer(call_timeout),
            data: Arc::new(RwLock::new(data)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            command_replay: Arc::new(Mutex::new(CommandReplay::new())),
            events,
            action_lock: Arc::new(tokio::sync::Mutex::new(())),
            update_lock: Arc::new(tokio::sync::Mutex::new(())),
            browser_auth_slots: Arc::new(Semaphore::new(MAX_PROVISIONAL_CONNECTIONS)),
            computer_auth_slots: Arc::new(Semaphore::new(MAX_PROVISIONAL_CONNECTIONS)),
        }
    }

    fn admit_command_call(&self, call_id: &str, fingerprint: &str) -> ReplayAdmission {
        self.command_replay
            .lock()
            .unwrap()
            .admit(call_id, fingerprint, Instant::now())
    }

    fn complete_command_call(&self, call_id: &str, status: StatusCode, body: Value) {
        self.command_replay
            .lock()
            .unwrap()
            .complete(call_id, status, body, Instant::now());
    }

    async fn refresh_update(&self) -> UpdateStatus {
        let _guard = self.update_lock.lock().await;
        {
            self.data.write().await.public.update = UpdateStatus::checking();
        }
        self.bump("update").await;

        let status = check_for_update().await;
        let log_status = match &status.status {
            UpdateState::Available => "warning",
            UpdateState::Error => "warning",
            _ => "ok",
        };
        self.log("update.check", log_status, &status.message).await;
        self.data.write().await.public.update = status.clone();
        self.bump("update").await;
        status
    }

    async fn log(&self, method: &str, status: &str, message: impl Into<String>) {
        let mut data = self.data.write().await;
        data.public.activity.push_front(Activity {
            id: Uuid::new_v4().simple().to_string()[..16].to_owned(),
            at: now_iso(),
            method: bounded(method, 80),
            status: bounded(status, 30),
            message: bounded(&message.into(), 1_000),
        });
        data.public.activity.truncate(MAX_ACTIVITY);
    }

    async fn bump(&self, event: &str) {
        let revision = {
            let mut data = self.data.write().await;
            data.public.revision = data.public.revision.saturating_add(1);
            data.public.revision
        };
        let _ = self.events.send(ServerEvent {
            name: event.to_owned(),
            revision,
        });
    }

    async fn log_for_connection(
        &self,
        hub: &ExtensionHub,
        connection_id: Uuid,
        method: &str,
        status: &str,
        message: impl Into<String>,
    ) -> bool {
        let mut data = self.data.write().await;
        if !hub.is_current_ready(connection_id) {
            return false;
        }
        data.public.activity.push_front(Activity {
            id: Uuid::new_v4().simple().to_string()[..16].to_owned(),
            at: now_iso(),
            method: bounded(method, 80),
            status: bounded(status, 30),
            message: bounded(&message.into(), 1_000),
        });
        data.public.activity.truncate(MAX_ACTIVITY);
        true
    }

    async fn bump_for_connection(
        &self,
        hub: &ExtensionHub,
        connection_id: Uuid,
        event: &str,
    ) -> bool {
        let revision = {
            let mut data = self.data.write().await;
            if !hub.is_current_ready(connection_id) {
                return false;
            }
            data.public.revision = data.public.revision.saturating_add(1);
            data.public.revision
        };
        let _ = self.events.send(ServerEvent {
            name: event.to_owned(),
            revision,
        });
        true
    }

    async fn public_state(&self) -> Value {
        let data = self.data.read().await;
        let mut value = serde_json::to_value(&data.public).unwrap_or_else(|_| json!({}));
        if let Some(observation) = value.get_mut("observation").and_then(Value::as_object_mut) {
            observation.insert(
                "screenshotUrl".to_owned(),
                data.screenshot
                    .as_ref()
                    .map(Screenshot::url)
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            insert_screenshot_metadata(observation, data.screenshot.as_ref());
        }
        if let Some(observation) = value
            .get_mut("computerObservation")
            .and_then(Value::as_object_mut)
        {
            observation.insert(
                "screenshotUrl".to_owned(),
                data.computer_screenshot
                    .as_ref()
                    .map(Screenshot::url)
                    .map(Value::String)
                    .unwrap_or(Value::Null),
            );
            insert_screenshot_metadata(observation, data.computer_screenshot.as_ref());
        }
        value
    }

    fn ensure_session(&self, headers: &HeaderMap) -> Result<(String, String), ApiError> {
        if let Some(id) = session_token(headers) {
            let csrf = self.touch_session(id)?;
            return Ok((csrf, id.to_owned()));
        }

        let supplied = bearer_token(headers);
        if !tokens_equal(supplied, &self.token) {
            return Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "Bridge token required to unlock the dashboard",
            ));
        }

        let now = OffsetDateTime::now_utc().unix_timestamp();
        let mut sessions = self.sessions.lock().unwrap();

        if sessions.len() >= 1_000 {
            let cutoff = now - 12 * 60 * 60;
            sessions.retain(|_, session| session.touched_at >= cutoff);
            while sessions.len() >= 800 {
                let Some(oldest) = sessions
                    .iter()
                    .min_by_key(|(_, session)| session.touched_at)
                    .map(|(id, _)| id.clone())
                else {
                    break;
                };
                sessions.remove(&oldest);
            }
        }

        let id = create_token();
        let csrf = create_token();
        sessions.insert(
            id.clone(),
            Session {
                csrf: csrf.clone(),
                touched_at: now,
            },
        );
        Ok((csrf, id))
    }

    fn assert_ui_read(&self, headers: &HeaderMap) -> Result<(), ApiError> {
        if tokens_equal(bearer_token(headers), &self.token) {
            return Ok(());
        }
        let id = session_token(headers).ok_or_else(|| {
            ApiError::new(
                StatusCode::UNAUTHORIZED,
                "UNAUTHORIZED",
                "Dashboard session required",
            )
        })?;
        self.touch_session(id).map(|_| ())
    }

    fn touch_session(&self, id: &str) -> Result<String, ApiError> {
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let mut sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get_mut(id)
            .filter(|session| session.touched_at >= now - 12 * 60 * 60)
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    "UNAUTHORIZED",
                    "Dashboard session expired",
                )
            })?;
        session.touched_at = now;
        Ok(session.csrf.clone())
    }

    fn assert_ui_mutation(&self, headers: &HeaderMap) -> Result<(), ApiError> {
        let id = session_token(headers)
            .ok_or_else(|| ApiError::forbidden("CSRF_REJECTED", "Invalid UI session"))?;
        let csrf = headers
            .get("x-csrf-token")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        let expected_csrf = self
            .touch_session(id)
            .map_err(|_| ApiError::forbidden("CSRF_REJECTED", "Invalid UI session"))?;
        if !tokens_equal(csrf, &expected_csrf) {
            return Err(ApiError::forbidden("CSRF_REJECTED", "Invalid UI session"));
        }

        let host = headers
            .get(HOST)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        let expected = format!("http://{host}");
        let origin = headers
            .get(ORIGIN)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        if origin != expected {
            return Err(ApiError::forbidden(
                "ORIGIN_REJECTED",
                "Cross-origin command rejected",
            ));
        }
        Ok(())
    }
}

#[derive(Default)]
struct StateData {
    public: PublicState,
    screenshot: Option<Screenshot>,
    computer_screenshot: Option<Screenshot>,
}

#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicState {
    revision: u64,
    connected: bool,
    extension: Option<ExtensionInfo>,
    browser_control: Value,
    computer_connected: bool,
    computer: Option<ComputerInfo>,
    target_tab_id: Option<u64>,
    tabs: Vec<TabInfo>,
    pending_dialog: Value,
    observation: Option<Observation>,
    computer_observation: Option<ComputerObservation>,
    activity: VecDeque<Activity>,
    update: UpdateStatus,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtensionInfo {
    version: String,
    protocol_version: u64,
    session_id: String,
    controller_id: String,
    connection_id: String,
    compatible: bool,
    browser: String,
    mode: String,
    capabilities: Vec<String>,
    connected_at: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComputerInfo {
    version: String,
    protocol_version: u64,
    session_id: String,
    compatible: bool,
    platform: String,
    architecture: String,
    backend: String,
    session_mode: String,
    isolation: String,
    input_ready: bool,
    semantic_ready: bool,
    capabilities: Vec<String>,
    windows: Vec<ComputerWindow>,
    share: Value,
    connected_at: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComputerWindow {
    id: String,
    pid: u64,
    app_name: String,
    title: String,
    width: u64,
    height: u64,
    focused: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComputerObservation {
    frame_id: String,
    captured_at: String,
    window_id: String,
    pid: u64,
    app_name: String,
    window_title: String,
    session_mode: String,
    delivery_mode: String,
    display_id: String,
    display_index: u64,
    display_name: String,
    image_width: u64,
    image_height: u64,
    screen_x: i64,
    screen_y: i64,
    screen_width: u64,
    screen_height: u64,
    scale_factor: f64,
    transport_scale_x: f64,
    transport_scale_y: f64,
    rotation: f64,
    semantic_mode: String,
    semantic_available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    semantic_error: Option<String>,
    pointer: ComputerPointer,
    elements: Vec<ComputerElement>,
    share: Value,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComputerPointer {
    id: String,
    visible: bool,
    window_id: Option<String>,
    image_x: Option<f64>,
    image_y: Option<f64>,
    screen_x: Option<i64>,
    screen_y: Option<i64>,
    heading_degrees: f64,
    action: String,
    pressed: bool,
    sequence: u64,
    revision: u64,
    buttons_mask: u8,
    updated_at: String,
    coordinate_space: String,
    style: Value,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ComputerElement {
    #[serde(rename = "ref")]
    reference: String,
    role: String,
    name: String,
    value: Option<String>,
    sensitive: bool,
    value_redacted: bool,
    enabled: Option<bool>,
    actions: Vec<String>,
    bounds: Option<ComputerElementBounds>,
    coordinate_space: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    screen_bounds: Option<ComputerElementBounds>,
}

#[derive(Clone, Serialize)]
struct ComputerElementBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Clone, Serialize)]
struct TabInfo {
    id: u64,
    title: String,
    url: String,
    active: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Observation {
    tab_id: u64,
    captured_at: String,
    title: String,
    url: String,
    generation: String,
    viewport: Value,
    scroll: Value,
    selected_text: String,
    body_text: String,
    elements: Vec<ElementInfo>,
    /// Cross-origin frames merged into `elements`. Absent (not empty) when
    /// the observation carries no frame provenance at all, so a frameless
    /// observation serializes exactly as it did before frame support.
    #[serde(skip_serializing_if = "Option::is_none")]
    frames: Option<Vec<FrameInfo>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frame_summary: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    elements_truncated: Option<bool>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ElementInfo {
    #[serde(rename = "ref")]
    reference: String,
    role: String,
    name: String,
    #[serde(rename = "type")]
    element_type: String,
    disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    checked: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    selected: Option<bool>,
    sensitive: bool,
    in_viewport: bool,
    risk: Option<String>,
    bounds: Option<Bounds>,
    /// Frame provenance, present only on elements that came from a merged
    /// cross-origin frame. `cross_origin` is never set without a `frame_ref`,
    /// so a page cannot forge cross-origin provenance onto a top element.
    #[serde(skip_serializing_if = "Option::is_none")]
    frame_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frame_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    frame_url_origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    cross_origin: Option<bool>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct FrameInfo {
    #[serde(rename = "ref")]
    reference: String,
    frame_id: String,
    url_origin: String,
    cross_origin: bool,
    depth: u64,
    offset: Offset,
    /// The owner iframe's size only. A `Bounds` here would publish an
    /// `x`/`y` pair of its own next to `offset`, which is the frame's actual
    /// position: two origins for one frame, one of them always zero.
    size: Size,
    element_count: u64,
    truncated: bool,
}

#[derive(Clone, Serialize)]
struct Offset {
    x: i64,
    y: i64,
}

#[derive(Clone, Serialize)]
struct Size {
    width: i64,
    height: i64,
}

#[derive(Clone, Serialize)]
struct Bounds {
    x: i64,
    y: i64,
    width: i64,
    height: i64,
}

#[derive(Clone, Serialize)]
struct Activity {
    id: String,
    at: String,
    method: String,
    status: String,
    message: String,
}

#[derive(Clone)]
struct Screenshot {
    bytes: Bytes,
    content_type: &'static str,
    content_hash: String,
    width: Option<u32>,
    height: Option<u32>,
    id: String,
    binding: String,
    route: &'static str,
}

impl Screenshot {
    fn bind(&mut self, route: &'static str, kind: &str, identity: &str) {
        self.route = route;
        self.binding = format!("{kind}.{}", URL_SAFE_NO_PAD.encode(identity.as_bytes()));
    }

    fn url(&self) -> String {
        format!("{}?id={}&binding={}", self.route, self.id, self.binding)
    }

    fn matches(&self, query: &HashMap<String, String>, route: &str) -> bool {
        self.route == route
            && query.get("id") == Some(&self.id)
            && query.get("binding") == Some(&self.binding)
    }
}

fn insert_screenshot_metadata(
    observation: &mut Map<String, Value>,
    screenshot: Option<&Screenshot>,
) {
    observation.insert(
        "contentHash".to_owned(),
        screenshot
            .map(|screenshot| Value::String(screenshot.content_hash.clone()))
            .unwrap_or(Value::Null),
    );
    observation.insert(
        "screenshotWidth".to_owned(),
        screenshot
            .and_then(|screenshot| screenshot.width)
            .map(|width| json!(width))
            .unwrap_or(Value::Null),
    );
    observation.insert(
        "screenshotHeight".to_owned(),
        screenshot
            .and_then(|screenshot| screenshot.height)
            .map(|height| json!(height))
            .unwrap_or(Value::Null),
    );
}

struct Session {
    csrf: String,
    touched_at: i64,
}

#[derive(Clone)]
struct ServerEvent {
    name: String,
    revision: u64,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    code: String,
    message: String,
}

impl ApiError {
    fn new(status: StatusCode, code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            status,
            code: code.into(),
            message: message.into(),
        }
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, "BAD_REQUEST", message)
    }

    fn forbidden(code: &str, message: &str) -> Self {
        Self::new(StatusCode::FORBIDDEN, code, message)
    }

    fn body(&self) -> Value {
        let taxonomy = classify(&self.code);
        json!({
            "ok": false,
            "error": { "code": self.code, "message": self.message },
            "taxonomy": {
                "code": taxonomy.code.as_str(),
                "retriable": taxonomy.retriable,
                "recoveryHint": taxonomy.recovery_hint.as_str(),
                "prose": taxonomy.prose,
            },
        })
    }
}

/// The few connector codes whose HTTP status is deliberately narrower than
/// their taxonomy class would give. Everything else takes the class status,
/// so this list is the entire set of exceptions and each one states why.
///
/// A code that agrees with its class must not be listed here: the unit tests
/// prove every entry really does differ from its class default, so a
/// reclassification cannot leave a stale override behind.
fn connector_status_override(code: &str) -> Option<StatusCode> {
    match code {
        // The caller sent a coordinate the connector proved is outside the
        // surface. Unlike the rest of `out_of_bounds`, the number itself is
        // wrong and a fresh observation is not what fixes it.
        "BAD_COORDINATES" => Some(StatusCode::BAD_REQUEST),
        // A missing operating-system permission is a standing refusal, not a
        // lock a human is holding for the length of one action: no handback
        // resumes it, so it stays a policy 403 rather than 423.
        "COMPUTER_PERMISSION_REQUIRED" => Some(StatusCode::FORBIDDEN),
        // page.handleDialog with no dialog recorded is well formed; it is the
        // page state, not the request, that does not match, and the identical
        // request becomes valid the moment a dialog opens.
        "NO_PENDING_DIALOG" => Some(StatusCode::CONFLICT),
        _ => None,
    }
}

impl From<HubError> for ApiError {
    /// Every failure a connector hands back takes its HTTP status from the
    /// taxonomy class the code already classifies into, so the status and the
    /// published `taxonomy` object can never disagree and no code can fall
    /// through to a 500 that blames the local server for a connector's state.
    /// The only exceptions are the narrower statuses listed above.
    fn from(error: HubError) -> Self {
        let status = connector_status_override(&error.code)
            .unwrap_or_else(|| classify(&error.code).code.http_status());
        Self::new(status, error.code, error.message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body())).into_response()
    }
}

/// Idempotency bookkeeping for `POST /api/v1/command` bodies carrying a
/// `callId`. All bearer commands share one principal because the bridge has
/// exactly one bearer token, so the registry is keyed by `callId` alone.
/// Every registration also pins the fingerprint of the request it was made
/// for, so a callId reused for a different command is refused instead of
/// silently replaying (or waiting on) the other command's outcome.
struct CommandReplay {
    in_flight: HashMap<String, String>,
    completed: HashMap<String, ReplayEntry>,
    ticks: u64,
}

struct ReplayEntry {
    fingerprint: String,
    status: StatusCode,
    body: Value,
    stored_at: Instant,
    last_used: u64,
}

enum ReplayAdmission {
    New,
    InFlight,
    Replay { status: StatusCode, body: Value },
    Reused,
}

impl CommandReplay {
    fn new() -> Self {
        Self {
            in_flight: HashMap::new(),
            completed: HashMap::new(),
            ticks: 0,
        }
    }

    /// Atomically admits one command for a callId: a cached completion with
    /// the same fingerprint replays, an in-flight duplicate is refused, a
    /// fingerprint mismatch is rejected as reuse, anything else registers.
    fn admit(&mut self, call_id: &str, fingerprint: &str, now: Instant) -> ReplayAdmission {
        self.evict_expired(now);
        self.ticks = self.ticks.saturating_add(1);
        let tick = self.ticks;
        if let Some(entry) = self.completed.get_mut(call_id) {
            if entry.fingerprint != fingerprint {
                return ReplayAdmission::Reused;
            }
            entry.last_used = tick;
            return ReplayAdmission::Replay {
                status: entry.status,
                body: entry.body.clone(),
            };
        }
        if let Some(registered) = self.in_flight.get(call_id) {
            return if registered == fingerprint {
                ReplayAdmission::InFlight
            } else {
                ReplayAdmission::Reused
            };
        }
        self.in_flight
            .insert(call_id.to_owned(), fingerprint.to_owned());
        ReplayAdmission::New
    }

    /// Stores the exact final response for a registered callId so later
    /// duplicates replay it without re-dispatching to any connector.
    fn complete(&mut self, call_id: &str, status: StatusCode, body: Value, now: Instant) {
        // A completion without a registration has no fingerprint to pin, so
        // it is dropped instead of stored as an unverifiable entry.
        let Some(fingerprint) = self.in_flight.remove(call_id) else {
            return;
        };
        self.evict_expired(now);
        while self.completed.len() >= REPLAY_CACHE_ENTRIES {
            let Some(oldest) = self
                .completed
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(id, _)| id.clone())
            else {
                break;
            };
            self.completed.remove(&oldest);
        }
        self.ticks = self.ticks.saturating_add(1);
        self.completed.insert(
            call_id.to_owned(),
            ReplayEntry {
                fingerprint,
                status,
                body,
                stored_at: now,
                last_used: self.ticks,
            },
        );
    }

    fn evict_expired(&mut self, now: Instant) {
        self.completed
            .retain(|_, entry| now.duration_since(entry.stored_at) < REPLAY_CACHE_TTL);
    }
}

/// Hashes one command request (method plus its canonical JSON parameters) so
/// a repeated callId can prove it carries the exact same command. serde_json
/// objects serialize with sorted keys, so equal parameter values always hash
/// identically.
fn command_fingerprint(method: &str, params: &Value) -> String {
    sha256_hex(format!("{method}\n{params}").as_bytes())
}

/// The synthetic cached outcome for an admitted callId whose handler future
/// was dropped (client disconnect, panic) before the real outcome was
/// recorded: the dispatched action may still execute, so the caller must
/// observe before deciding whether to act again.
fn interrupted_call_error() -> ApiError {
    ApiError::new(
        StatusCode::GATEWAY_TIMEOUT,
        "COMMAND_OUTCOME_UNKNOWN",
        "The original command with this callId was interrupted before its outcome was recorded; observe the current state before deciding whether to act again",
    )
}

/// Completes an admitted callId with a synthetic outcome-unknown failure if
/// the handler never reaches its own completion. The command may already be
/// dispatched to a connector when the handler future is dropped, so freeing
/// the callId would let a retry re-dispatch the action; caching the
/// outcome-unknown failure makes every later replay of that callId return it
/// without touching any connector.
struct InFlightCallGuard {
    replay: Arc<Mutex<CommandReplay>>,
    call_id: Option<String>,
}

impl InFlightCallGuard {
    fn disarm(&mut self) -> Option<String> {
        self.call_id.take()
    }
}

impl Drop for InFlightCallGuard {
    fn drop(&mut self) {
        if let Some(call_id) = self.call_id.take()
            && let Ok(mut replay) = self.replay.lock()
        {
            let error = interrupted_call_error();
            let mut body = error.body();
            if let Some(object) = body.as_object_mut() {
                object.insert("callId".to_owned(), Value::String(call_id.clone()));
            }
            replay.complete(&call_id, error.status, body, Instant::now());
        }
    }
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/api/session", get(api_session))
        .route("/api/state", get(api_state))
        .route("/api/screenshot", get(api_screenshot))
        .route("/api/computer/screenshot", get(api_computer_screenshot))
        .route("/api/events", get(api_events))
        .route("/api/action", post(api_action))
        .route("/api/update/check", post(api_update_check))
        .route("/api/v1/command", post(api_command))
        .route("/bridge", get(websocket_upgrade))
        .route("/computer", get(computer_websocket_upgrade))
        .fallback(get(static_asset))
        .layer(axum::extract::DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            loopback_and_security_headers,
        ))
        .with_state(state)
}

async fn loopback_and_security_headers(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let host = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let mut response = if is_loopback_host(host, state.bound_port) {
        next.run(request).await
    } else {
        state
            .log(
                "host.guard",
                "warning",
                format!(
                    "Rejected a request whose Host header is not this loopback server: {}",
                    bounded(host, 200)
                ),
            )
            .await;
        ApiError::forbidden("HOST_REJECTED", "Only loopback hosts are accepted").into_response()
    };
    apply_security_headers(response.headers_mut());
    response
}

fn apply_security_headers(headers: &mut HeaderMap) {
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'self'; img-src 'self' data:; connect-src 'self'; script-src 'self'; style-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'self'"),
    );
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        "cross-origin-resource-policy",
        HeaderValue::from_static("same-origin"),
    );
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

async fn health(State(state): State<AppState>) -> Json<Value> {
    Json(json!({
        "ok": true,
        "extensionConnected": state.hub.connected(),
        "computerConnected": state.computer_hub.connected(),
        "version": VERSION,
    }))
}

async fn api_session(State(state): State<AppState>, headers: HeaderMap) -> Response {
    match state.ensure_session(&headers) {
        Ok((csrf, session_token)) => Json(json!({
            "ok": true,
            "csrfToken": csrf,
            "sessionToken": session_token,
            "expiresAfterIdleSeconds": 43_200,
        }))
        .into_response(),
        Err(error) => error.into_response(),
    }
}

async fn api_state(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    state.assert_ui_read(&headers)?;
    let public = state.public_state().await;
    Ok(Json(json!({ "ok": true, "state": public })).into_response())
}

async fn api_screenshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    state.assert_ui_read(&headers)?;
    let screenshot = state.data.read().await.screenshot.clone().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "NO_SCREENSHOT",
            "No screenshot has been captured",
        )
    })?;
    if !screenshot.matches(&query, "/api/screenshot") {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "STALE_SCREENSHOT",
            "The requested browser screenshot is no longer the exact current observation",
        ));
    }
    let mut response = Response::new(Body::from(screenshot.bytes.clone()));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static(screenshot.content_type),
    );
    response.headers_mut().insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&screenshot.bytes.len().to_string()).unwrap(),
    );
    Ok(response)
}

async fn api_computer_screenshot(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> Result<Response, ApiError> {
    state.assert_ui_read(&headers)?;
    let screenshot = state
        .data
        .read()
        .await
        .computer_screenshot
        .clone()
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::NOT_FOUND,
                "NO_COMPUTER_SCREENSHOT",
                "No computer screenshot has been captured",
            )
        })?;
    if !screenshot.matches(&query, "/api/computer/screenshot") {
        return Err(ApiError::new(
            StatusCode::CONFLICT,
            "STALE_SCREENSHOT",
            "The requested computer screenshot is no longer the exact current frame",
        ));
    }
    let mut response = Response::new(Body::from(screenshot.bytes.clone()));
    response.headers_mut().insert(
        CONTENT_TYPE,
        HeaderValue::from_static(screenshot.content_type),
    );
    response.headers_mut().insert(
        CONTENT_LENGTH,
        HeaderValue::from_str(&screenshot.bytes.len().to_string()).unwrap(),
    );
    Ok(response)
}

async fn api_events(State(state): State<AppState>, headers: HeaderMap) -> Response {
    if let Err(error) = state.assert_ui_read(&headers) {
        return error.into_response();
    }
    let revision = state.data.read().await.public.revision;
    let initial = stream::once(async move {
        Ok::<_, Infallible>(
            Event::default()
                .event("state")
                .data(json!({ "revision": revision }).to_string()),
        )
    });
    let updates = BroadcastStream::new(state.events.subscribe()).filter_map(|message| async move {
        match message {
            Ok(message) => Some(Ok::<_, Infallible>(
                Event::default()
                    .event(message.name)
                    .data(json!({ "revision": message.revision }).to_string()),
            )),
            Err(_) => None,
        }
    });
    Sse::new(initial.chain(updates))
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("heartbeat"),
        )
        .into_response()
}

async fn api_action(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    state.assert_ui_mutation(&headers)?;
    let body = parse_json_body(&body)?;
    let method = required_string(body.get("method"), "method", 80)?;
    let result = state
        .perform_action(
            &method,
            body.get("params").cloned().unwrap_or_else(|| json!({})),
        )
        .await?;
    let public = state.public_state().await;
    Ok(Json(json!({ "ok": true, "result": result, "state": public })).into_response())
}

async fn api_update_check(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    state.assert_ui_mutation(&headers)?;
    let update = state.refresh_update().await;
    let public = state.public_state().await;
    Ok(Json(json!({ "ok": true, "update": update, "state": public })).into_response())
}

async fn api_command(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, ApiError> {
    let supplied = bearer_token(&headers);
    if !tokens_equal(supplied, &state.token) {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "Bearer token required",
        ));
    }
    assert_command_origin(&headers)?;
    assert_json_content_type(&headers)?;
    let body = parse_json_body(&body)?;
    let method = required_string(body.get("method"), "method", 80)?;
    let params = body.get("params").cloned().unwrap_or_else(|| json!({}));
    let call_id = optional_call_id(body.get("callId"))?;

    if let Some(call_id) = call_id.as_deref() {
        match state.admit_command_call(call_id, &command_fingerprint(&method, &params)) {
            ReplayAdmission::New => {}
            ReplayAdmission::InFlight => {
                let error = ApiError::new(
                    StatusCode::CONFLICT,
                    "CALL_IN_PROGRESS",
                    "A command with this callId is still in flight; wait for its outcome",
                );
                let mut body = error.body();
                if let Some(object) = body.as_object_mut() {
                    object.insert("callId".to_owned(), Value::String(call_id.to_owned()));
                }
                return Ok((error.status, Json(body)).into_response());
            }
            ReplayAdmission::Replay { status, mut body } => {
                if let Some(object) = body.as_object_mut() {
                    object.insert("replayed".to_owned(), Value::Bool(true));
                }
                return Ok((status, Json(body)).into_response());
            }
            ReplayAdmission::Reused => {
                let error = ApiError::new(
                    StatusCode::CONFLICT,
                    "CALL_ID_REUSED",
                    "This callId was already used for a different command; use a fresh callId for each distinct command",
                );
                let mut body = error.body();
                if let Some(object) = body.as_object_mut() {
                    object.insert("callId".to_owned(), Value::String(call_id.to_owned()));
                }
                return Ok((error.status, Json(body)).into_response());
            }
        }
    }

    let mut registration = InFlightCallGuard {
        replay: state.command_replay.clone(),
        call_id: call_id.clone(),
    };
    let outcome = state.perform_action(&method, params).await;
    let (status, mut response_body) = match outcome {
        Ok(result) => {
            let public = state.public_state().await;
            (
                StatusCode::OK,
                json!({ "ok": true, "result": result, "state": public }),
            )
        }
        Err(error) => (error.status, error.body()),
    };
    if let Some(call_id) = registration.disarm() {
        if let Some(object) = response_body.as_object_mut() {
            object.insert("callId".to_owned(), Value::String(call_id.clone()));
        }
        state.complete_command_call(&call_id, status, response_body.clone());
    }
    Ok((status, Json(response_body)).into_response())
}

fn optional_call_id(value: Option<&Value>) -> Result<Option<String>, ApiError> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let call_id = required_string(Some(value), "callId", CALL_ID_MAX_CHARS)?;
            if call_id.is_empty() {
                return Err(ApiError::bad_request(
                    "callId must be between 1 and 128 characters",
                ));
            }
            Ok(Some(call_id))
        }
    }
}

fn assert_command_origin(headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(origin) = headers.get(ORIGIN).and_then(|value| value.to_str().ok()) else {
        return Ok(());
    };
    let host = headers
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if origin != format!("http://{host}") {
        return Err(ApiError::forbidden(
            "ORIGIN_REJECTED",
            "Cross-origin command rejected",
        ));
    }
    Ok(())
}

fn assert_json_content_type(headers: &HeaderMap) -> Result<(), ApiError> {
    let content_type = headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let media_type = content_type.split(';').next().unwrap_or("").trim();
    if !media_type.eq_ignore_ascii_case("application/json") {
        return Err(ApiError::new(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "UNSUPPORTED_MEDIA_TYPE",
            "Commands require Content-Type: application/json",
        ));
    }
    Ok(())
}

async fn websocket_upgrade(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let origin = headers
        .get(ORIGIN)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if !valid_extension_origin(origin) {
        return Err(ApiError::forbidden(
            "ORIGIN_REJECTED",
            "Extension Origin required",
        ));
    }
    reject_legacy_websocket_credentials(&headers, &query)?;
    let permit = state
        .browser_auth_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "AUTH_BUSY",
                "Too many provisional extension connections",
            )
        })?;
    Ok(ws
        .max_message_size(MAX_WS_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_websocket(socket, state, permit))
        .into_response())
}

fn valid_extension_origin(origin: &str) -> bool {
    let Some(extension_id) = origin.strip_prefix("chrome-extension://") else {
        return false;
    };
    extension_id.len() == 32
        && extension_id
            .bytes()
            .all(|byte| (b'a'..=b'p').contains(&byte))
}

fn reject_legacy_websocket_credentials(
    headers: &HeaderMap,
    query: &HashMap<String, String>,
) -> Result<(), ApiError> {
    if !query.is_empty() || headers.contains_key(AUTHORIZATION) {
        return Err(ApiError::new(
            StatusCode::BAD_REQUEST,
            "LEGACY_WEBSOCKET_CREDENTIAL_REJECTED",
            "WebSocket authentication must not use query or Authorization credentials",
        ));
    }
    Ok(())
}

async fn computer_websocket_upgrade(
    State(state): State<AppState>,
    Query(query): Query<HashMap<String, String>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, ApiError> {
    let origin = headers
        .get(ORIGIN)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    if origin != COMPUTER_HELPER_ORIGIN {
        return Err(ApiError::forbidden(
            "ORIGIN_REJECTED",
            "Computer helper Origin required",
        ));
    }
    reject_legacy_websocket_credentials(&headers, &query)?;
    let permit = state
        .computer_auth_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "AUTH_BUSY",
                "Too many provisional computer helper connections",
            )
        })?;
    Ok(ws
        .max_message_size(MAX_WS_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_computer_websocket(socket, state, permit))
        .into_response())
}

fn session_envelope_valid(message: &Value, connection_id: Uuid) -> bool {
    let expected_session = connection_id.to_string();
    message.get("protocolVersion").and_then(Value::as_u64) == Some(PROTOCOL_VERSION)
        && message.get("sessionId").and_then(Value::as_str) == Some(expected_session.as_str())
}

fn session_event_valid(message: &Value, connection_id: Uuid, last_sequence: &mut u64) -> bool {
    if !session_envelope_valid(message, connection_id) {
        return false;
    }
    let Some(sequence) = message.get("eventSequence").and_then(Value::as_u64) else {
        return false;
    };
    if sequence <= *last_sequence {
        return false;
    }
    *last_sequence = sequence;
    true
}

async fn authenticate_websocket(
    socket: &mut WebSocket,
    token: &str,
    connector: &'static str,
) -> Result<Uuid, &'static str> {
    let deadline = tokio::time::Instant::now() + AUTH_TIMEOUT;
    let mut challenge: Option<ServerChallenge> = None;

    for _ in 0..MAX_AUTH_MESSAGES {
        let message = tokio::time::timeout_at(deadline, socket.recv())
            .await
            .map_err(|_| "authentication response timed out")?
            .ok_or("authentication peer closed")?
            .map_err(|_| "authentication socket read failed")?;
        match message {
            Message::Text(text) => {
                if text.len() > MAX_AUTH_MESSAGE_BYTES {
                    return Err("authentication response was too large");
                }
                let response = serde_json::from_str::<Value>(text.as_str())
                    .map_err(|_| "authentication response was not valid JSON")?;
                if let Some(challenge) = challenge.as_ref() {
                    challenge
                        .verify_response(token, &response)
                        .map_err(|_| "authentication response proof failed")?;
                    return Ok(challenge.session_id());
                }
                let next_challenge =
                    ServerChallenge::from_client_hello(token, connector, &response)
                        .map_err(|_| "authentication hello was invalid")?;
                let challenge_text = next_challenge.envelope().to_string();
                tokio::time::timeout_at(
                    deadline,
                    socket.send(Message::Text(challenge_text.into())),
                )
                .await
                .map_err(|_| "authentication challenge send timed out")?
                .map_err(|_| "authentication challenge send failed")?;
                challenge = Some(next_challenge);
            }
            Message::Ping(bytes) => {
                tokio::time::timeout_at(deadline, socket.send(Message::Pong(bytes)))
                    .await
                    .map_err(|_| "authentication pong timed out")?
                    .map_err(|_| "authentication pong failed")?;
            }
            Message::Pong(_) => {}
            Message::Close(_) => return Err("authentication peer closed"),
            Message::Binary(_) => return Err("binary authentication messages are forbidden"),
        }
    }
    Err("authentication message limit exceeded")
}

async fn reject_provisional_socket(mut socket: WebSocket) {
    let _ = socket.send(Message::Close(None)).await;
}

async fn handle_websocket(
    mut socket: WebSocket,
    state: AppState,
    auth_permit: OwnedSemaphorePermit,
) {
    let connection_id =
        match authenticate_websocket(&mut socket, &state.token, BROWSER_CONNECTOR).await {
            Ok(connection_id) => connection_id,
            Err(_) => {
                reject_provisional_socket(socket).await;
                return;
            }
        };
    drop(auth_permit);
    let (connection_id, mut outgoing) = {
        let mut data = state.data.write().await;
        let connection = state.hub.attach_with_id(connection_id);
        data.public.connected = false;
        data.public.extension = None;
        data.public.browser_control = Value::Null;
        data.public.pending_dialog = Value::Null;
        data.public.tabs.clear();
        data.public.target_tab_id = None;
        data.public.observation = None;
        data.screenshot = None;
        connection
    };
    state
        .log(
            "bridge",
            "info",
            "Browser extension transport connected; awaiting handshake",
        )
        .await;
    state.bump("connection").await;
    let _ = state.hub.send_to(
        connection_id,
        json!({
            "type": "welcome",
            "protocolVersion": PROTOCOL_VERSION,
            "sessionId": connection_id.to_string(),
            "serverVersion": VERSION,
            "connector": "browser-extension"
        }),
    );

    let (mut socket_sender, mut socket_receiver) = socket.split();
    let writer = tokio::spawn(async move {
        while let Some(message) = outgoing.recv().await {
            let closing = matches!(message, Message::Close(_));
            if socket_sender.send(message).await.is_err() || closing {
                break;
            }
        }
    });

    let mut handshake_complete = false;
    let mut last_event_sequence = 0_u64;
    while let Some(Ok(message)) = socket_receiver.next().await {
        match message {
            Message::Text(text) => {
                let Ok(message) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                match message.get("type").and_then(Value::as_str) {
                    Some("ping")
                        if handshake_complete
                            && state.hub.is_current_ready(connection_id)
                            && session_envelope_valid(&message, connection_id) =>
                    {
                        let _ = state.hub.send_to(
                            connection_id,
                            json!({
                                "type": "pong",
                                "protocolVersion": PROTOCOL_VERSION,
                                "sessionId": connection_id.to_string()
                            }),
                        );
                    }
                    Some("hello") => {
                        handshake_complete = handle_hello(&state, connection_id, &message).await;
                    }
                    Some("event")
                        if handshake_complete
                            && state.hub.is_current_ready(connection_id)
                            && session_event_valid(
                                &message,
                                connection_id,
                                &mut last_event_sequence,
                            ) =>
                    {
                        handle_extension_event(&state, connection_id, &message).await
                    }
                    _ if handshake_complete
                        && state.hub.is_current_ready(connection_id)
                        && message.get("type").and_then(Value::as_str) == Some("result")
                        && message.get("id").and_then(Value::as_str).is_some()
                        && state.hub.resolve(connection_id, &message) =>
                    {
                        state
                            .log(
                                "bridge",
                                "error",
                                "Browser extension sent a mismatched result envelope; connection revoked",
                            )
                            .await;
                        break;
                    }
                    _ => {}
                }
            }
            Message::Ping(bytes) => {
                let _ = state
                    .hub
                    .send_message_to(connection_id, Message::Pong(bytes));
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    writer.abort();
    let detached = {
        let mut data = state.data.write().await;
        let detached = state.hub.detach(connection_id);
        if detached {
            data.public.connected = false;
            data.public.extension = None;
            data.public.browser_control = Value::Null;
            data.public.pending_dialog = Value::Null;
            data.public.tabs.clear();
            data.public.target_tab_id = None;
            data.public.observation = None;
            data.screenshot = None;
        }
        detached
    };
    if detached {
        state
            .log("bridge", "warning", "Browser extension disconnected")
            .await;
        state.bump("connection").await;
    }
}

async fn handle_computer_websocket(
    mut socket: WebSocket,
    state: AppState,
    auth_permit: OwnedSemaphorePermit,
) {
    let connection_id =
        match authenticate_websocket(&mut socket, &state.token, COMPUTER_CONNECTOR).await {
            Ok(connection_id) => connection_id,
            Err(_) => {
                reject_provisional_socket(socket).await;
                return;
            }
        };
    drop(auth_permit);
    let (connection_id, mut outgoing) = {
        let mut data = state.data.write().await;
        let connection = state.computer_hub.attach_with_id(connection_id);
        data.public.computer_connected = false;
        data.public.computer = None;
        data.public.computer_observation = None;
        data.computer_screenshot = None;
        connection
    };
    state
        .log(
            "computer.bridge",
            "info",
            "Computer helper transport connected; awaiting handshake",
        )
        .await;
    state.bump("computer-connection").await;
    let _ = state.computer_hub.send_to(
        connection_id,
        json!({
            "type": "welcome",
            "protocolVersion": PROTOCOL_VERSION,
            "sessionId": connection_id.to_string(),
            "serverVersion": VERSION,
            "connector": "computer-helper"
        }),
    );

    let (mut socket_sender, mut socket_receiver) = socket.split();
    let writer = tokio::spawn(async move {
        while let Some(message) = outgoing.recv().await {
            let closing = matches!(message, Message::Close(_));
            if socket_sender.send(message).await.is_err() || closing {
                break;
            }
        }
    });

    let mut handshake_complete = false;
    let mut last_event_sequence = 0_u64;
    while let Some(Ok(message)) = socket_receiver.next().await {
        match message {
            Message::Text(text) => {
                let Ok(message) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                match message.get("type").and_then(Value::as_str) {
                    Some("ping")
                        if handshake_complete
                            && state.computer_hub.is_current_ready(connection_id)
                            && session_envelope_valid(&message, connection_id) =>
                    {
                        let _ = state.computer_hub.send_to(
                            connection_id,
                            json!({
                                "type": "pong",
                                "protocolVersion": PROTOCOL_VERSION,
                                "sessionId": connection_id.to_string()
                            }),
                        );
                    }
                    Some("hello") => {
                        handshake_complete =
                            handle_computer_hello(&state, connection_id, &message).await;
                    }
                    Some("event")
                        if handshake_complete
                            && state.computer_hub.is_current_ready(connection_id)
                            && session_event_valid(
                                &message,
                                connection_id,
                                &mut last_event_sequence,
                            ) =>
                    {
                        handle_computer_event(&state, connection_id, &message).await
                    }
                    _ if handshake_complete
                        && state.computer_hub.is_current_ready(connection_id)
                        && message.get("type").and_then(Value::as_str) == Some("result")
                        && message.get("id").and_then(Value::as_str).is_some()
                        && state.computer_hub.resolve(connection_id, &message) =>
                    {
                        state
                            .log(
                                "computer.bridge",
                                "error",
                                "Computer helper sent a mismatched result envelope; connection revoked",
                            )
                            .await;
                        break;
                    }
                    _ => {}
                }
            }
            Message::Ping(bytes) => {
                let _ = state
                    .computer_hub
                    .send_message_to(connection_id, Message::Pong(bytes));
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    writer.abort();
    let detached = {
        let mut data = state.data.write().await;
        let detached = state.computer_hub.detach(connection_id);
        if detached {
            data.public.computer_connected = false;
            data.public.computer = None;
            data.public.computer_observation = None;
            data.computer_screenshot = None;
        }
        detached
    };
    if detached {
        state
            .log("computer.bridge", "warning", "Computer helper disconnected")
            .await;
        state.bump("computer-connection").await;
    }
}

async fn handle_computer_hello(state: &AppState, connection_id: Uuid, message: &Value) -> bool {
    if !state.computer_hub.is_current(connection_id) {
        return false;
    }
    let version = bounded(
        message
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        50,
    );
    let protocol_version = message
        .get("protocolVersion")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let session_id = bounded(
        message
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or(""),
        80,
    );
    let envelope_compatible = version == VERSION
        && protocol_version == PROTOCOL_VERSION
        && session_id == connection_id.to_string();
    let compatible = envelope_compatible && state.computer_hub.mark_ready(connection_id);
    let capabilities: Vec<String> = message
        .get("capabilities")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|item| {
                    COMPUTER_METHODS.contains(item) || *item == COMPUTER_SHARE_ACK_CAPABILITY
                })
                .take(COMPUTER_METHODS.len() + 1)
                .map(|item| item.to_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
        .into_iter()
        .filter(|_| compatible)
        .collect();
    let share_ack_paced = capabilities
        .iter()
        .any(|item| item == COMPUTER_SHARE_ACK_CAPABILITY);
    let computer = ComputerInfo {
        version: version.clone(),
        protocol_version,
        session_id: session_id.clone(),
        compatible,
        platform: bounded(
            message
                .get("platform")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            50,
        ),
        architecture: bounded(
            message
                .get("architecture")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            50,
        ),
        backend: bounded(
            message
                .get("backend")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            100,
        ),
        session_mode: bounded(
            message
                .get("sessionMode")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            80,
        ),
        isolation: "foreground-and-hardware-cursor-preserved".to_owned(),
        input_ready: compatible
            && message
                .get("inputReady")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        semantic_ready: compatible
            && message
                .get("semanticReady")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        capabilities,
        windows: sanitize_computer_windows(message.get("windows")),
        share: sanitize_share_status(message.get("share")),
        connected_at: now_iso(),
    };
    {
        let mut data = state.data.write().await;
        if !state.computer_hub.is_current(connection_id) {
            return false;
        }
        data.public.computer_connected = compatible;
        data.public.computer = Some(computer);
    }
    if !compatible {
        state
            .log(
                "computer.bridge",
                "warning",
                format!(
                    "Helper handshake rejected (version {version}, protocol {protocol_version}, session match {})",
                    session_id == connection_id.to_string()
                ),
            )
            .await;
    } else {
        state
            .log(
                "computer.bridge",
                "ok",
                "Computer helper handshake complete",
            )
            .await;
    }
    let _ = state.computer_hub.send_to(connection_id, json!({
        "type": "helloAck",
        "protocolVersion": PROTOCOL_VERSION,
        "sessionId": connection_id.to_string(),
        "ok": compatible,
        // Confirms the negotiated share-frame ack pacing so a helper that did
        // not advertise the capability keeps the legacy timer behavior and is
        // never sent an eventAck it does not understand.
        "shareAck": share_ack_paced,
        "error": (!compatible).then(|| json!({
            "code": "COMPUTER_PROTOCOL_MISMATCH",
            "message": format!("Helper and server must both use package {VERSION} and protocol {PROTOCOL_VERSION}")
        }))
    }));
    state.bump("computer-hello").await;
    compatible
}

async fn handle_hello(state: &AppState, connection_id: Uuid, message: &Value) -> bool {
    if !state.hub.is_current(connection_id) {
        return false;
    }
    let version = bounded(
        message
            .get("version")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        50,
    );
    let protocol_version = message
        .get("protocolVersion")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let session_id = bounded(
        message
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or(""),
        80,
    );
    let controller_id = bounded(
        message
            .get("controllerId")
            .and_then(Value::as_str)
            .unwrap_or(""),
        100,
    );
    let client_connection_id = bounded(
        message
            .get("connectionId")
            .and_then(Value::as_str)
            .unwrap_or(""),
        100,
    );
    let envelope_compatible = version == VERSION
        && protocol_version == PROTOCOL_VERSION
        && session_id == connection_id.to_string()
        && !controller_id.is_empty()
        && !client_connection_id.is_empty();
    let compatible = envelope_compatible && state.hub.mark_ready(connection_id);
    let extension = ExtensionInfo {
        version: version.clone(),
        protocol_version,
        session_id: session_id.clone(),
        controller_id,
        connection_id: client_connection_id,
        compatible,
        browser: bounded(
            message
                .get("browser")
                .and_then(Value::as_str)
                .unwrap_or("Chromium"),
            100,
        ),
        mode: if message.get("mode").and_then(Value::as_str) == Some("full-access") {
            "full-access".to_owned()
        } else {
            "safe".to_owned()
        },
        capabilities: message
            .get("capabilities")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(Value::as_str)
                    .filter(|item| ACTION_METHODS.contains(item))
                    .take(ACTION_METHODS.len())
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
            .into_iter()
            .filter(|_| compatible)
            .collect(),
        connected_at: now_iso(),
    };
    {
        let mut data = state.data.write().await;
        if !state.hub.is_current(connection_id) {
            return false;
        }
        data.public.connected = compatible;
        data.public.extension = Some(extension);
    }
    let _ = state.hub.send_to(connection_id, json!({
        "type": "helloAck",
        "protocolVersion": PROTOCOL_VERSION,
        "sessionId": connection_id.to_string(),
        "ok": compatible,
        "error": (!compatible).then(|| json!({
            "code": "EXTENSION_PROTOCOL_MISMATCH",
            "message": format!("Extension and server must both use package {VERSION} and protocol {PROTOCOL_VERSION}")
        }))
    }));
    state.bump("hello").await;

    if !compatible {
        state
            .log(
                "bridge",
                "warning",
                format!(
                    "Extension handshake rejected (version {version}, protocol {protocol_version}, session match {})",
                    session_id == connection_id.to_string()
                ),
            )
            .await;
        return false;
    }
    state
        .log("bridge", "ok", "Browser extension handshake complete")
        .await;

    let state = state.clone();
    tokio::spawn(async move {
        let _guard = state.action_lock.lock().await;
        if let Err(error) = state.refresh_tabs_for(Some(connection_id)).await {
            state.log("tabs.list", "warning", error.message).await;
            state.bump("warning").await;
        }
    });
    true
}

async fn handle_computer_event(state: &AppState, connection_id: Uuid, message: &Value) {
    let name = message
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let data = message.get("data").unwrap_or(&Value::Null);
    match name {
        "computer.share.frame" => {
            let mut screenshot = match decode_screenshot(data.get("screenshot")) {
                Ok(Some(screenshot)) => screenshot,
                Ok(None) => {
                    // The frame is discarded, but an ack-paced helper must
                    // still be unblocked or the share would stall forever.
                    acknowledge_computer_share_frame(state, connection_id, data).await;
                    return;
                }
                Err(error) => {
                    if state
                        .log_for_connection(
                            &state.computer_hub,
                            connection_id,
                            name,
                            "warning",
                            error.message,
                        )
                        .await
                    {
                        state
                            .bump_for_connection(
                                &state.computer_hub,
                                connection_id,
                                "computer-share-error",
                            )
                            .await;
                    }
                    acknowledge_computer_share_frame(state, connection_id, data).await;
                    return;
                }
            };
            let observation = match sanitize_computer_observation(data.get("frame")) {
                Ok(observation) => observation,
                Err(error) => {
                    if state
                        .log_for_connection(
                            &state.computer_hub,
                            connection_id,
                            name,
                            "warning",
                            error.message,
                        )
                        .await
                    {
                        state
                            .bump_for_connection(
                                &state.computer_hub,
                                connection_id,
                                "computer-share-error",
                            )
                            .await;
                    }
                    acknowledge_computer_share_frame(state, connection_id, data).await;
                    return;
                }
            };
            screenshot.bind(
                "/api/computer/screenshot",
                "computer-frame",
                &observation.frame_id,
            );
            {
                let mut state_data = state.data.write().await;
                if !state.computer_hub.is_current_ready(connection_id) {
                    return;
                }
                if let Some(computer) = state_data.public.computer.as_mut()
                    && let Some(frame) = data.get("frame")
                {
                    computer.share = sanitize_share_status(frame.get("share"));
                }
                state_data.public.computer_observation = Some(observation);
                state_data.computer_screenshot = Some(screenshot);
            }
            acknowledge_computer_share_frame(state, connection_id, data).await;
            state
                .bump_for_connection(&state.computer_hub, connection_id, "computer-share-frame")
                .await;
        }
        "computer.share.error" => {
            let detail = bounded(
                data.get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("Computer share capture failed"),
                500,
            );
            if state
                .log_for_connection(&state.computer_hub, connection_id, name, "warning", detail)
                .await
            {
                state
                    .bump_for_connection(&state.computer_hub, connection_id, "computer-share-error")
                    .await;
            }
        }
        _ => {}
    }
}

/// Share sequence a frame event is acknowledged under, read from the raw
/// payload so even a frame the server could not store still unblocks the
/// helper's ack-paced mailbox.
fn share_frame_ack_sequence(data: &Value) -> Option<u64> {
    data.get("frame")?.get("share")?.get("sequence")?.as_u64()
}

/// Sends the `eventAck` for one processed `computer.share.frame` event.
///
/// The acknowledgement is version-gated on the hello capability intersection:
/// only a helper that advertised `computer.share.ack` ever receives one, so a
/// legacy helper keeps its timer behavior and never sees an unknown message.
async fn acknowledge_computer_share_frame(state: &AppState, connection_id: Uuid, data: &Value) {
    let Some(sequence) = share_frame_ack_sequence(data) else {
        return;
    };
    let negotiated = state
        .data
        .read()
        .await
        .public
        .computer
        .as_ref()
        .is_some_and(|computer| {
            computer
                .capabilities
                .iter()
                .any(|item| item == COMPUTER_SHARE_ACK_CAPABILITY)
        });
    if !negotiated {
        return;
    }
    let _ = state.computer_hub.send_to(
        connection_id,
        json!({
            "type": "eventAck",
            "protocolVersion": PROTOCOL_VERSION,
            "sessionId": connection_id.to_string(),
            "name": "computer.share.frame",
            "sequence": sequence,
        }),
    );
}

async fn handle_extension_event(state: &AppState, connection_id: Uuid, message: &Value) {
    let name = message
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let data = message.get("data").cloned().unwrap_or_else(|| json!({}));
    match name {
        "browser.control.started" => {
            {
                let mut state_data = state.data.write().await;
                if !state.hub.is_current_ready(connection_id) {
                    return;
                }
                state_data.public.browser_control = sanitize_browser_control(&data);
                // A fresh lease starts without a blocking JavaScript dialog.
                state_data.public.pending_dialog = Value::Null;
            }
            if state
                .log_for_connection(
                    &state.hub,
                    connection_id,
                    "browser.control",
                    "ok",
                    "Browser control session started",
                )
                .await
            {
                state
                    .bump_for_connection(&state.hub, connection_id, "browser-control")
                    .await;
            }
        }
        "browser.control.revoked" => {
            {
                let mut state_data = state.data.write().await;
                if !state.hub.is_current_ready(connection_id) {
                    return;
                }
                state_data.public.browser_control = json!({
                    "active": false,
                    "revocation": sanitize_control_revocation(&data)
                });
                // Revoking the lease detaches the debugger, so any recorded
                // dialog can no longer be handled through the bridge.
                state_data.public.pending_dialog = Value::Null;
            }
            let reason = bounded(
                data.get("reason")
                    .and_then(Value::as_str)
                    .unwrap_or("control_revoked"),
                100,
            );
            if state
                .log_for_connection(
                    &state.hub,
                    connection_id,
                    "browser.control",
                    "warning",
                    format!("Browser control revoked: {reason}"),
                )
                .await
            {
                state
                    .bump_for_connection(&state.hub, connection_id, "browser-control")
                    .await;
            }
        }
        "approval.resolved" => {
            let ok = data.get("ok").and_then(Value::as_bool) == Some(true);
            let detail = if ok {
                "Human approved and executed the action".to_owned()
            } else {
                data.get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("Approval failed")
                    .to_owned()
            };
            if !state
                .log_for_connection(
                    &state.hub,
                    connection_id,
                    "approval",
                    if ok { "ok" } else { "error" },
                    detail,
                )
                .await
            {
                return;
            }
            if !state
                .bump_for_connection(&state.hub, connection_id, "approval")
                .await
            {
                return;
            }
            if ok {
                let state = state.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    if !state.hub.is_current_ready(connection_id) {
                        return;
                    }
                    let _guard = state.action_lock.lock().await;
                    if !state.hub.is_current_ready(connection_id) {
                        return;
                    }
                    let _ = state.refresh_tabs_for(Some(connection_id)).await;
                    if !state.hub.is_current_ready(connection_id) {
                        return;
                    }
                    let target = state.data.read().await.public.target_tab_id;
                    if data.get("method").and_then(Value::as_str) != Some("tabs.close")
                        && let Some(tab_id) = target
                    {
                        let _ = state
                            .refresh_observation_for(tab_id, Some(connection_id))
                            .await;
                    }
                });
            }
        }
        "approval.rejected" => {
            let logged = state
                .log_for_connection(
                    &state.hub,
                    connection_id,
                    "approval",
                    "warning",
                    "Human rejected the pending action",
                )
                .await;
            if !logged {
                return;
            }
            state
                .bump_for_connection(&state.hub, connection_id, "approval")
                .await;
        }
        "page.dialogOpened" => {
            let dialog = sanitize_pending_dialog(&data);
            let kind = dialog
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("dialog")
                .to_owned();
            {
                let mut state_data = state.data.write().await;
                if !state.hub.is_current_ready(connection_id) {
                    return;
                }
                state_data.public.pending_dialog = dialog;
            }
            if state
                .log_for_connection(
                    &state.hub,
                    connection_id,
                    "page.dialog",
                    "warning",
                    format!(
                        "JavaScript {kind} dialog opened; browser commands are blocked until page.handleDialog resolves it"
                    ),
                )
                .await
            {
                state
                    .bump_for_connection(&state.hub, connection_id, "dialog")
                    .await;
            }
        }
        "page.dialogClosed" => {
            {
                let mut state_data = state.data.write().await;
                if !state.hub.is_current_ready(connection_id) {
                    return;
                }
                state_data.public.pending_dialog = Value::Null;
            }
            if state
                .log_for_connection(
                    &state.hub,
                    connection_id,
                    "page.dialog",
                    "ok",
                    "JavaScript dialog closed",
                )
                .await
            {
                state
                    .bump_for_connection(&state.hub, connection_id, "dialog")
                    .await;
            }
        }
        _ => {}
    }
}

/// Sanitizes the extension's dialog-opened event payload into the published
/// `pendingDialog` shape; anything unexpected collapses to bounded defaults.
fn sanitize_pending_dialog(data: &Value) -> Value {
    let kind = data
        .get("type")
        .and_then(Value::as_str)
        .filter(|kind| matches!(*kind, "alert" | "confirm" | "prompt" | "beforeunload"))
        .unwrap_or("dialog");
    json!({
        "type": kind,
        "message": bounded(data.get("message").and_then(Value::as_str).unwrap_or(""), 500),
        "hasPrompt": data
            .get("hasPrompt")
            .and_then(Value::as_bool)
            .unwrap_or(kind == "prompt"),
        "at": data.get("at").and_then(Value::as_u64),
        "tabId": data.get("tabId").and_then(Value::as_u64),
    })
}

/// The commands that stay usable while a JavaScript dialog is blocking the
/// controlled page; every other browser command—including the read-only
/// `page.observe` and `page.waitFor`, whose content-script calls would hang
/// against the dialog-frozen renderer and revoke the lease by timeout—fails
/// fast server-side with `BLOCKED_BY_DIALOG` and is never relayed to the
/// extension. Every exempt method stays off the renderer main thread:
/// `status`, `tabs.list`, and `browser.control.status` read browser-process
/// state, `browser.control.stop`'s overlay hide is best-effort, and
/// `page.handleDialog` resolves the dialog through browser-side CDP.
fn dialog_tolerant_method(method: &str) -> bool {
    matches!(
        method,
        "status"
            | "tabs.list"
            | "browser.control.status"
            | "browser.control.stop"
            | "page.handleDialog"
    )
}

async fn static_asset(request: Request) -> Result<Response, ApiError> {
    let path = match request.uri().path() {
        "/" => "index.html",
        "/demo" => "demo.html",
        path => path.trim_start_matches('/'),
    };
    let file = PUBLIC_DIR
        .get_file(path)
        .ok_or_else(|| ApiError::new(StatusCode::NOT_FOUND, "NOT_FOUND", "Not found"))?;
    let content_type = match path.rsplit('.').next() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        _ => "application/octet-stream",
    };
    let mut response = Response::new(Body::from(file.contents()));
    response
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    Ok(response)
}

impl AppState {
    async fn perform_action(&self, method: &str, raw_params: Value) -> Result<Value, ApiError> {
        if COMPUTER_METHODS.contains(&method) {
            return self.perform_computer_action(method, raw_params).await;
        }
        if !ACTION_METHODS.contains(&method) {
            return Err(ApiError::bad_request("Unsupported action"));
        }
        let _guard = self.action_lock.lock().await;
        let extension = self.data.read().await.public.extension.clone();
        let Some(extension) = extension else {
            return Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "EXTENSION_HANDSHAKE_PENDING",
                "Browser extension handshake is not complete",
            ));
        };
        if !extension.compatible {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "EXTENSION_PROTOCOL_MISMATCH",
                format!(
                    "Extension {} protocol {} does not match server {VERSION} protocol {PROTOCOL_VERSION}",
                    extension.version, extension.protocol_version
                ),
            ));
        }
        if !extension.capabilities.iter().any(|item| item == method) {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "EXTENSION_CAPABILITY_UNAVAILABLE",
                format!("Browser extension did not advertise {method}"),
            ));
        }
        let (target_tab_id, browser_control, viewport, pending_dialog) = {
            let data = self.data.read().await;
            (
                data.public.target_tab_id,
                data.public.browser_control.clone(),
                data.public.observation.as_ref().and_then(|observation| {
                    let width = observation.viewport.get("width").and_then(Value::as_f64)?;
                    let height = observation.viewport.get("height").and_then(Value::as_f64)?;
                    Some((width, height))
                }),
                data.public.pending_dialog.clone(),
            )
        };
        // A recorded JavaScript dialog blocks every mutating browser command
        // before sanitization or relay: the page cannot answer until a human
        // or page.handleDialog resolves the dialog.
        if !pending_dialog.is_null() && !dialog_tolerant_method(method) {
            let kind = pending_dialog
                .get("type")
                .and_then(Value::as_str)
                .unwrap_or("dialog");
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "BLOCKED_BY_DIALOG",
                format!(
                    "A JavaScript {kind} dialog is blocking the page; resolve it with page.handleDialog (accept:false is the safe default for beforeunload) before {method} can run"
                ),
            ));
        }
        let mut params = sanitize_params(method, raw_params, target_tab_id, viewport)?;
        // page.waitFor is read-only and runs without a control lease, so it
        // never carries control bindings.
        if method.starts_with("page.") && !matches!(method, "page.observe" | "page.waitFor") {
            // page.handleDialog is the only way out of a blocking dialog, and
            // the only way to refresh an observation turn is an observation
            // the dialog itself forbids. An observation discarded mid-flight
            // leaves the extension a turn ahead of the published state, so
            // binding the escape hatch to a turn would deadlock exactly the
            // situation it exists for. It keeps the session binding, which is
            // what actually proves the dialog belongs to this lease.
            let bind_freshness = method != "page.handleDialog";
            bind_browser_control(&mut params, &browser_control, bind_freshness);
        }

        if method == "tabs.list" {
            return self.refresh_tabs().await;
        }
        if method == "page.observe" {
            return self
                .refresh_observation(params["tabId"].as_u64().unwrap())
                .await;
        }

        let (connection_id, result) = match self.hub.call_scoped(method, params.clone()).await {
            Ok(result) => result,
            Err(error) => {
                self.log(method, "error", &error.message).await;
                self.bump("error").await;
                return Err(error.into());
            }
        };

        let returned_control = if method.starts_with("browser.control.") {
            Some(sanitize_browser_control(&result))
        } else {
            result.get("control").map(sanitize_browser_control)
        };
        {
            let mut data = self.data.write().await;
            if !self.hub.is_current_ready(connection_id) {
                return Ok(result);
            }
            if let Some(control) = returned_control.as_ref() {
                data.public.browser_control = control.clone();
            }
            if method == "page.handleDialog" {
                // The extension acknowledged Page.handleJavaScriptDialog, so
                // the dialog gate lifts immediately; the dialogClosed event
                // clears it again idempotently.
                data.public.pending_dialog = Value::Null;
            }
            if method == "tabs.activate" {
                data.public.target_tab_id = params.get("tabId").and_then(Value::as_u64);
            } else if method == "tabs.new"
                && let Some(tab_id) = result.get("tabId").and_then(Value::as_u64)
            {
                data.public.target_tab_id = Some(tab_id);
            }
        }
        if returned_control.is_some()
            && !self
                .bump_for_connection(&self.hub, connection_id, "browser-control")
                .await
        {
            return Ok(result);
        }

        if result.get("status").and_then(Value::as_str) == Some("approval_required") {
            let risk = result
                .get("risk")
                .and_then(Value::as_str)
                .unwrap_or("Sensitive action");
            if self
                .log_for_connection(
                    &self.hub,
                    connection_id,
                    method,
                    "approval",
                    format!("{risk}; approve it in the extension popup"),
                )
                .await
            {
                self.bump_for_connection(&self.hub, connection_id, "approval")
                    .await;
            }
            return Ok(result);
        }

        let log_message = if method == "page.fill" {
            format!(
                "Filled {}",
                params.get("ref").and_then(Value::as_str).unwrap_or("field")
            )
        } else {
            format!("{method} completed")
        };
        if !self
            .log_for_connection(&self.hub, connection_id, method, "ok", log_message)
            .await
        {
            return Ok(result);
        }

        if method.starts_with("tabs.")
            && let Err(error) = self.refresh_tabs_for(Some(connection_id)).await
        {
            self.log("tabs.list", "warning", &error.message).await;
            self.bump("warning").await;
        }

        if let Some(delay) = observation_delay(method) {
            tokio::time::sleep(delay).await;
            let (target, dialog_pending) = {
                let data = self.data.read().await;
                (
                    data.public.target_tab_id,
                    !data.public.pending_dialog.is_null(),
                )
            };
            // A dialog opened by the just-executed action freezes the
            // renderer, so observing now would only time out and revoke the
            // lease; the observation resumes once the dialog is resolved.
            if !dialog_pending
                && let Some(tab_id) = target
                && let Err(error) = self
                    .refresh_observation_for(tab_id, Some(connection_id))
                    .await
            {
                // The dialog can also open in the gap between the check above
                // and the extension receiving the relayed observation. The
                // extension then refuses it with BLOCKED_BY_DIALOG, which is a
                // skipped observation and nothing more: the published
                // observation, the screenshot, and the lease all stay exactly
                // as they were, and the client resumes after
                // page.handleDialog.
                if error.code == "BLOCKED_BY_DIALOG" {
                    self.log(
                        "page.observe",
                        "warning",
                        "Automatic observation skipped: a JavaScript dialog opened while it was in flight; resolve it with page.handleDialog",
                    )
                    .await;
                    self.bump("dialog").await;
                } else {
                    self.log("page.observe", "warning", &error.message).await;
                    self.bump("warning").await;
                }
            }
        }

        Ok(result)
    }

    async fn perform_computer_action(
        &self,
        method: &str,
        raw_params: Value,
    ) -> Result<Value, ApiError> {
        let _guard = self.action_lock.lock().await;
        let (computer, pointer_revision, frame_size) = {
            let data = self.data.read().await;
            (
                data.public.computer.clone(),
                data.public
                    .computer_observation
                    .as_ref()
                    .map(|observation| observation.pointer.sequence),
                data.public
                    .computer_observation
                    .as_ref()
                    .map(|observation| (observation.image_width, observation.image_height)),
            )
        };
        let mut params = sanitize_computer_params(method, raw_params, frame_size)?;
        let Some(computer) = computer else {
            return Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "COMPUTER_HANDSHAKE_PENDING",
                "Computer helper handshake is not complete",
            ));
        };
        if !computer.compatible {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "COMPUTER_PROTOCOL_MISMATCH",
                format!(
                    "Computer helper {} protocol {} does not match server {VERSION} protocol {PROTOCOL_VERSION}",
                    computer.version, computer.protocol_version
                ),
            ));
        }
        if !computer.capabilities.iter().any(|item| item == method) {
            return Err(ApiError::new(
                StatusCode::CONFLICT,
                "COMPUTER_CAPABILITY_UNAVAILABLE",
                format!("Computer helper did not advertise {method}"),
            ));
        }
        if method == "computer.observe" {
            return self
                .refresh_computer_observation(
                    params
                        .get("windowId")
                        .or_else(|| params.get("displayId"))
                        .and_then(Value::as_str),
                )
                .await;
        }
        if params.get("frameId").is_some()
            && params.get("expectedPointerRevision").is_none()
            && let Some(pointer_revision) = pointer_revision
            && let Some(output) = params.as_object_mut()
        {
            output.insert(
                "expectedPointerRevision".to_owned(),
                json!(pointer_revision),
            );
        }

        let (connection_id, result) =
            match self.computer_hub.call_scoped(method, params.clone()).await {
                Ok(result) => result,
                Err(error) => {
                    self.log(method, "error", &error.message).await;
                    self.bump("computer-error").await;
                    return Err(error.into());
                }
            };

        if method == "computer.status" {
            let mut data = self.data.write().await;
            if !self.computer_hub.is_current_ready(connection_id) {
                return Ok(result);
            }
            if let Some(computer) = data.public.computer.as_mut() {
                if let Some(input_ready) = result.get("inputReady").and_then(Value::as_bool) {
                    computer.input_ready = input_ready;
                }
                if let Some(semantic_ready) = result.get("semanticReady").and_then(Value::as_bool) {
                    computer.semantic_ready = semantic_ready;
                }
                computer.session_mode = bounded(
                    result
                        .get("sessionMode")
                        .and_then(Value::as_str)
                        .unwrap_or(&computer.session_mode),
                    80,
                );
                computer.isolation = bounded(
                    result
                        .get("isolation")
                        .and_then(Value::as_str)
                        .unwrap_or(&computer.isolation),
                    100,
                );
                computer.windows = sanitize_computer_windows(result.get("windows"));
                computer.share = sanitize_share_status(result.get("share"));
            }
            drop(data);
            self.log_for_connection(
                &self.computer_hub,
                connection_id,
                method,
                "ok",
                "Computer helper status refreshed",
            )
            .await;
            self.bump_for_connection(&self.computer_hub, connection_id, "computer-status")
                .await;
            return Ok(result);
        }

        if method.starts_with("computer.share.") {
            let mut data = self.data.write().await;
            if !self.computer_hub.is_current_ready(connection_id) {
                return Ok(result);
            }
            if let Some(computer) = data.public.computer.as_mut() {
                computer.share = sanitize_share_status(Some(&result));
            }
            drop(data);
            if !self
                .log_for_connection(
                    &self.computer_hub,
                    connection_id,
                    method,
                    "ok",
                    format!("{method} completed"),
                )
                .await
            {
                return Ok(result);
            }
            self.bump_for_connection(&self.computer_hub, connection_id, "computer-share")
                .await;
            if method == "computer.share.start" {
                let window_id = result
                    .get("windowId")
                    .and_then(Value::as_str)
                    .or_else(|| params.get("windowId").and_then(Value::as_str));
                let _ = self
                    .refresh_computer_observation_for(window_id, Some(connection_id))
                    .await;
            }
            return Ok(result);
        }

        if !self
            .log_for_connection(
                &self.computer_hub,
                connection_id,
                method,
                "ok",
                format!("{method} completed"),
            )
            .await
        {
            return Ok(result);
        }
        tokio::time::sleep(computer_observation_delay(method)).await;
        let window_id = self
            .data
            .read()
            .await
            .public
            .computer_observation
            .as_ref()
            .map(|observation| observation.window_id.clone());
        if let Err(error) = self
            .refresh_computer_observation_for(window_id.as_deref(), Some(connection_id))
            .await
        {
            self.log("computer.observe", "warning", error.message).await;
            self.bump("computer-warning").await;
        }
        Ok(result)
    }

    async fn refresh_computer_observation(
        &self,
        window_id: Option<&str>,
    ) -> Result<Value, ApiError> {
        self.refresh_computer_observation_for(window_id, None).await
    }

    async fn refresh_computer_observation_for(
        &self,
        window_id: Option<&str>,
        expected_connection_id: Option<Uuid>,
    ) -> Result<Value, ApiError> {
        let params = window_id
            .map(|window_id| json!({ "windowId": window_id }))
            .unwrap_or_else(|| json!({}));
        let (connection_id, result) = self
            .computer_hub
            .call_scoped("computer.observe", params)
            .await
            .map_err(ApiError::from)?;
        if expected_connection_id.is_some_and(|expected| expected != connection_id) {
            return Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "COMPUTER_DISCONNECTED",
                "Computer helper was replaced before the observation started",
            ));
        }
        let mut screenshot = decode_screenshot(result.get("screenshot"))?.ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "COMPUTER_INVALID_OBSERVATION",
                "Computer helper returned no screenshot",
            )
        })?;
        let observation = sanitize_computer_observation(result.get("frame"))?;
        screenshot.bind(
            "/api/computer/screenshot",
            "computer-frame",
            &observation.frame_id,
        );
        {
            let mut data = self.data.write().await;
            if !self.computer_hub.is_current_ready(connection_id) {
                return Err(ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "COMPUTER_DISCONNECTED",
                    "Computer helper was replaced before the observation could be committed",
                ));
            }
            data.public.computer_observation = Some(observation.clone());
            data.computer_screenshot = Some(screenshot);
        }
        if self
            .log_for_connection(
                &self.computer_hub,
                connection_id,
                "computer.observe",
                "ok",
                format!(
                    "Observed {} — {} in background",
                    observation.app_name, observation.window_title
                ),
            )
            .await
        {
            self.bump_for_connection(&self.computer_hub, connection_id, "computer-observation")
                .await;
        }
        Ok(serde_json::to_value(observation).unwrap_or_else(|_| json!({})))
    }

    async fn refresh_tabs(&self) -> Result<Value, ApiError> {
        self.refresh_tabs_for(None).await
    }

    async fn refresh_tabs_for(
        &self,
        expected_connection_id: Option<Uuid>,
    ) -> Result<Value, ApiError> {
        let (connection_id, result) = self
            .hub
            .call_scoped("tabs.list", json!({}))
            .await
            .map_err(ApiError::from)?;
        if expected_connection_id.is_some_and(|expected| expected != connection_id) {
            return Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "EXTENSION_DISCONNECTED",
                "Browser extension was replaced before tabs could be refreshed",
            ));
        }
        let tabs = result
            .get("tabs")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(sanitize_tab)
                    .take(500)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let active_tab_id = result.get("activeTabId").and_then(Value::as_u64);
        {
            let mut data = self.data.write().await;
            if !self.hub.is_current_ready(connection_id) {
                return Err(ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "EXTENSION_DISCONNECTED",
                    "Browser extension was replaced before tabs could be committed",
                ));
            }
            data.public.tabs = tabs;
            let target_exists = data
                .public
                .target_tab_id
                .is_some_and(|target| data.public.tabs.iter().any(|tab| tab.id == target));
            if !target_exists {
                data.public.target_tab_id = active_tab_id
                    .filter(|active| data.public.tabs.iter().any(|tab| tab.id == *active))
                    .or_else(|| data.public.tabs.first().map(|tab| tab.id));
                data.public.observation = None;
                data.screenshot = None;
            }
        }
        self.bump_for_connection(&self.hub, connection_id, "tabs")
            .await;
        Ok(result)
    }

    async fn refresh_observation(&self, tab_id: u64) -> Result<Value, ApiError> {
        self.refresh_observation_for(tab_id, None).await
    }

    async fn refresh_observation_for(
        &self,
        tab_id: u64,
        expected_connection_id: Option<Uuid>,
    ) -> Result<Value, ApiError> {
        let (connection_id, result) = self
            .hub
            .call_scoped("page.observe", json!({ "tabId": tab_id }))
            .await
            .map_err(ApiError::from)?;
        if expected_connection_id.is_some_and(|expected| expected != connection_id) {
            return Err(ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                "EXTENSION_DISCONNECTED",
                "Browser extension was replaced before the observation started",
            ));
        }
        let mut screenshot = decode_screenshot(result.get("screenshot"))?;
        let browser_control = result.get("control").map(sanitize_browser_control);
        let snapshot = result.get("snapshot").and_then(Value::as_object);
        let raw_elements = snapshot
            .and_then(|item| item.get("elements"))
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        let (elements, elements_truncated) = publish_elements(&raw_elements);
        // Frame provenance is additive: a frameless observation leaves all
        // three fields absent and serializes byte-identically to 0.10.
        let frames = snapshot
            .and_then(|item| item.get("frames"))
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .filter_map(sanitize_frame)
                    .take(MAX_OBSERVED_FRAMES)
                    .collect::<Vec<FrameInfo>>()
            })
            .map(|frames| reconcile_frame_counts(frames, &elements))
            .filter(|frames| !frames.is_empty());
        let frame_summary = snapshot
            .and_then(|item| item.get("frameSummary"))
            .map(sanitize_frame_summary)
            .filter(|summary| !summary.is_null());
        let observation = Observation {
            tab_id,
            captured_at: now_iso(),
            title: bounded(
                snapshot
                    .and_then(|item| item.get("title"))
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                500,
            ),
            url: bounded(
                snapshot
                    .and_then(|item| item.get("url"))
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                4_096,
            ),
            generation: bounded(
                snapshot
                    .and_then(|item| item.get("generation"))
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                100,
            ),
            viewport: snapshot
                .and_then(|item| item.get("viewport"))
                .cloned()
                .unwrap_or(Value::Null),
            scroll: snapshot
                .and_then(|item| item.get("scroll"))
                .cloned()
                .unwrap_or(Value::Null),
            selected_text: bounded(
                snapshot
                    .and_then(|item| item.get("selectedText"))
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                5_000,
            ),
            body_text: bounded(
                snapshot
                    .and_then(|item| item.get("bodyText"))
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                20_000,
            ),
            elements,
            frames,
            frame_summary,
            elements_truncated: elements_truncated.then_some(true),
        };
        if let Some(screenshot) = screenshot.as_mut() {
            screenshot.bind(
                "/api/screenshot",
                "browser-tab-generation",
                &format!("{tab_id}:{}", observation.generation),
            );
        }
        {
            let mut data = self.data.write().await;
            if !self.hub.is_current_ready(connection_id) {
                return Err(ApiError::new(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "EXTENSION_DISCONNECTED",
                    "Browser extension was replaced before the observation could be committed",
                ));
            }
            data.public.target_tab_id = Some(tab_id);
            data.public.observation = Some(observation);
            data.screenshot = screenshot;
            if let Some(control) = browser_control {
                data.public.browser_control = control;
            }
        }
        if self
            .log_for_connection(
                &self.hub,
                connection_id,
                "page.observe",
                "ok",
                format!("Observed tab {tab_id}"),
            )
            .await
        {
            self.bump_for_connection(&self.hub, connection_id, "observation")
                .await;
        }
        Ok(result)
    }
}

fn observation_delay(method: &str) -> Option<Duration> {
    Some(Duration::from_millis(match method {
        "tabs.activate" => 150,
        "page.navigate" => 700,
        "page.back" | "page.forward" => 500,
        "page.reload" => 600,
        // A whole batch settles like a click: one auto-observe afterwards.
        "page.click" | "page.clickAt" | "page.batch" => 350,
        "page.fill" | "page.typeText" => 100,
        "page.select" | "page.hover" => 200,
        "page.key" => 250,
        "page.handleDialog" => 300,
        "page.scroll" | "page.evaluate" => 150,
        _ => return None,
    }))
}

fn computer_observation_delay(method: &str) -> Duration {
    Duration::from_millis(match method {
        "computer.typeText" | "computer.key" => 120,
        "computer.scroll" | "computer.move" => 180,
        "computer.click" => 350,
        "computer.drag" => 450,
        "computer.invoke" | "computer.setValue" => 250,
        _ => 150,
    })
}

fn sanitize_computer_params(
    method: &str,
    input: Value,
    frame_size: Option<(u64, u64)>,
) -> Result<Value, ApiError> {
    let source = input.as_object().cloned().unwrap_or_default();
    if !COMPUTER_METHODS.contains(&method) {
        return Err(ApiError::bad_request("Unsupported computer action"));
    }
    let computer_extents = || {
        normalized_extents(
            &source,
            "image",
            frame_size.map(|(width, height)| (width as f64, height as f64)),
            (
                "NO_COMPUTER_FRAME",
                "Observe the computer first; normalized1000 coordinates need a current frame with valid image dimensions",
            ),
        )
    };
    match method {
        "computer.status" | "computer.share.status" | "computer.share.stop" => Ok(json!({})),
        "computer.share.start" => {
            let window_id = required_string(source.get("windowId"), "windowId", 100)?;
            let fps = source
                .get("fps")
                .map(|value| as_u64(value, "fps"))
                .transpose()?
                .unwrap_or(4);
            if !(1..=10).contains(&fps) {
                return Err(ApiError::bad_request("fps must be between 1 and 10"));
            }
            Ok(json!({ "windowId": window_id, "fps": fps }))
        }
        "computer.observe" => {
            let Some(window_id) = source.get("windowId").or_else(|| source.get("displayId")) else {
                return Ok(json!({}));
            };
            Ok(json!({
                "windowId": required_string(Some(window_id), "windowId", 100)?
            }))
        }
        "computer.move" | "computer.click" | "computer.scroll" => {
            let extents = computer_extents()?;
            let mut output = computer_frame_params(&source)?;
            output.insert(
                "x".to_owned(),
                json!(scaled_coordinate(
                    computer_coordinate(&source, "x")?,
                    "x",
                    extents.map(|(width, _)| width),
                )?),
            );
            output.insert(
                "y".to_owned(),
                json!(scaled_coordinate(
                    computer_coordinate(&source, "y")?,
                    "y",
                    extents.map(|(_, height)| height),
                )?),
            );
            if matches!(method, "computer.move" | "computer.click")
                && let Some(duration) = source.get("durationMs")
            {
                let duration = as_u64(duration, "durationMs")?;
                if !(50..=2_000).contains(&duration) {
                    return Err(ApiError::bad_request(
                        "durationMs must be between 50 and 2000",
                    ));
                }
                output.insert("durationMs".to_owned(), json!(duration));
            }
            if method == "computer.click" {
                let button = optional_string(source.get("button"), "left", "button", 8)?;
                if !["left", "middle", "right"].contains(&button.as_str()) {
                    return Err(ApiError::bad_request(
                        "button must be left, middle, or right",
                    ));
                }
                let click_count = source
                    .get("clickCount")
                    .map(|value| as_u64(value, "clickCount"))
                    .transpose()?
                    .unwrap_or(1);
                if !(1..=3).contains(&click_count) {
                    return Err(ApiError::bad_request("clickCount must be between 1 and 3"));
                }
                output.insert("button".to_owned(), json!(button));
                output.insert("clickCount".to_owned(), json!(click_count));
            } else if method == "computer.scroll" {
                let delta_x = finite_number(source.get("deltaX"), 0.0, "deltaX")?
                    .trunc()
                    .clamp(-50.0, 50.0) as i64;
                let delta_y = finite_number(source.get("deltaY"), 0.0, "deltaY")?
                    .trunc()
                    .clamp(-50.0, 50.0) as i64;
                output.insert("deltaX".to_owned(), json!(delta_x));
                output.insert("deltaY".to_owned(), json!(delta_y));
            }
            Ok(Value::Object(output))
        }
        "computer.drag" => {
            let extents = computer_extents()?;
            let mut output = computer_frame_params(&source)?;
            for name in ["fromX", "fromY", "toX", "toY"] {
                let extent = extents.map(
                    |(width, height)| {
                        if name.ends_with('X') { width } else { height }
                    },
                );
                output.insert(
                    name.to_owned(),
                    json!(scaled_coordinate(
                        computer_coordinate(&source, name)?,
                        name,
                        extent,
                    )?),
                );
            }
            let duration_ms = source
                .get("durationMs")
                .map(|value| as_u64(value, "durationMs"))
                .transpose()?
                .unwrap_or(500);
            if !(50..=2_000).contains(&duration_ms) {
                return Err(ApiError::bad_request(
                    "durationMs must be between 50 and 2000",
                ));
            }
            output.insert("durationMs".to_owned(), json!(duration_ms));
            Ok(Value::Object(output))
        }
        "computer.typeText" => {
            let mut output = computer_frame_params(&source)?;
            output.insert(
                "text".to_owned(),
                Value::String(required_string(source.get("text"), "text", 100_000)?),
            );
            Ok(Value::Object(output))
        }
        "computer.key" => {
            let mut output = computer_frame_params(&source)?;
            let key = required_string(source.get("key"), "key", 200)?;
            output.insert("key".to_owned(), Value::String(normalize_key_chord(&key)?));
            Ok(Value::Object(output))
        }
        "computer.invoke" => {
            let mut output = computer_frame_params(&source)?;
            output.insert(
                "elementRef".to_owned(),
                Value::String(required_string(source.get("elementRef"), "elementRef", 40)?),
            );
            let action = optional_string(source.get("action"), "press", "action", 40)?;
            if !["press", "showMenu", "pick", "confirm", "cancel", "open"]
                .contains(&action.as_str())
            {
                return Err(ApiError::bad_request("Unsupported semantic action"));
            }
            output.insert("action".to_owned(), Value::String(action));
            Ok(Value::Object(output))
        }
        "computer.setValue" => {
            let mut output = computer_frame_params(&source)?;
            output.insert(
                "elementRef".to_owned(),
                Value::String(required_string(source.get("elementRef"), "elementRef", 40)?),
            );
            output.insert(
                "value".to_owned(),
                Value::String(required_string(source.get("value"), "value", 100_000)?),
            );
            Ok(Value::Object(output))
        }
        _ => Err(ApiError::bad_request("Unsupported computer action")),
    }
}

fn computer_frame_params(source: &Map<String, Value>) -> Result<Map<String, Value>, ApiError> {
    let mut output = Map::from_iter([(
        "frameId".to_owned(),
        Value::String(required_string(source.get("frameId"), "frameId", 100)?),
    )]);
    if let Some(value) = source.get("expectedPointerRevision") {
        output.insert(
            "expectedPointerRevision".to_owned(),
            json!(as_u64(value, "expectedPointerRevision")?),
        );
    }
    Ok(output)
}

fn computer_coordinate(source: &Map<String, Value>, name: &str) -> Result<f64, ApiError> {
    let value = finite_number(source.get(name), f64::NAN, name)?;
    if !(0.0..=100_000.0).contains(&value) {
        return Err(ApiError::bad_request(format!(
            "{name} must be between 0 and 100000"
        )));
    }
    Ok(value)
}

/// Resolves the optional `coordinateSpace` request field to conversion
/// extents. `None` means the default space: coordinates pass through
/// untouched. `Some` carries the width and height that normalized1000
/// coordinates convert against; a missing or degenerate observation is
/// rejected so a model cannot act on coordinates the server cannot ground.
fn normalized_extents(
    source: &Map<String, Value>,
    default_space: &str,
    frame_size: Option<(f64, f64)>,
    missing: (&str, &str),
) -> Result<Option<(f64, f64)>, ApiError> {
    let space = optional_string(
        source.get("coordinateSpace"),
        default_space,
        "coordinateSpace",
        40,
    )?;
    if space == default_space {
        return Ok(None);
    }
    if space != "normalized1000" {
        return Err(ApiError::bad_request(format!(
            "coordinateSpace must be {default_space} or normalized1000"
        )));
    }
    let usable = |extent: f64| extent.is_finite() && extent >= 1.0;
    let (code, message) = missing;
    match frame_size {
        Some((width, height)) if usable(width) && usable(height) => Ok(Some((width, height))),
        _ => Err(ApiError::new(StatusCode::CONFLICT, code, message)),
    }
}

/// Converts one coordinate to pixels when a normalized1000 extent applies;
/// the caller has already validated the raw value as a finite non-negative
/// number, so only the 0..=1000 envelope is enforced here.
fn scaled_coordinate(value: f64, name: &str, extent: Option<f64>) -> Result<f64, ApiError> {
    let Some(extent) = extent else {
        return Ok(value);
    };
    if !(0.0..=NORMALIZED_COORDINATE_MAX).contains(&value) {
        return Err(ApiError::bad_request(format!(
            "{name} must be between 0 and 1000 in the normalized1000 coordinate space"
        )));
    }
    Ok(normalized1000_to_pixels(value, extent))
}

/// Pure normalized1000 -> pixel conversion, clamped to the last addressable
/// pixel: the boundary value 1000 must convert to `extent - 1`, because
/// every downstream validator rejects coordinates at or beyond the extent.
fn normalized1000_to_pixels(value: f64, extent: f64) -> f64 {
    (value / NORMALIZED_COORDINATE_MAX * extent).clamp(0.0, (extent - 1.0).max(0.0))
}

fn sanitize_computer_observation(value: Option<&Value>) -> Result<ComputerObservation, ApiError> {
    let frame = value.and_then(Value::as_object).ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "COMPUTER_INVALID_OBSERVATION",
            "Computer helper returned invalid frame metadata",
        )
    })?;
    let bounded_u64 = |name: &str, max: u64| -> Result<u64, ApiError> {
        let value = frame.get(name).and_then(Value::as_u64).ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "COMPUTER_INVALID_OBSERVATION",
                format!("Computer frame {name} is invalid"),
            )
        })?;
        if value == 0 || value > max {
            return Err(ApiError::new(
                StatusCode::BAD_GATEWAY,
                "COMPUTER_INVALID_OBSERVATION",
                format!("Computer frame {name} is out of range"),
            ));
        }
        Ok(value)
    };
    let signed = |name: &str| -> Result<i64, ApiError> {
        let value = frame.get(name).and_then(Value::as_i64).ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "COMPUTER_INVALID_OBSERVATION",
                format!("Computer frame {name} is invalid"),
            )
        })?;
        if !(-100_000..=100_000).contains(&value) {
            return Err(ApiError::new(
                StatusCode::BAD_GATEWAY,
                "COMPUTER_INVALID_OBSERVATION",
                format!("Computer frame {name} is out of range"),
            ));
        }
        Ok(value)
    };
    let finite = |name: &str, min: f64, max: f64| -> Result<f64, ApiError> {
        let value = frame.get(name).and_then(Value::as_f64).ok_or_else(|| {
            ApiError::new(
                StatusCode::BAD_GATEWAY,
                "COMPUTER_INVALID_OBSERVATION",
                format!("Computer frame {name} is invalid"),
            )
        })?;
        if !value.is_finite() || !(min..=max).contains(&value) {
            return Err(ApiError::new(
                StatusCode::BAD_GATEWAY,
                "COMPUTER_INVALID_OBSERVATION",
                format!("Computer frame {name} is out of range"),
            ));
        }
        Ok(value)
    };
    let window_id = required_string(
        frame.get("windowId").or_else(|| frame.get("displayId")),
        "frame.windowId",
        100,
    )?;
    let display_name = bounded(
        frame
            .get("displayName")
            .and_then(Value::as_str)
            .unwrap_or("Window"),
        200,
    );
    Ok(ComputerObservation {
        frame_id: required_string(frame.get("id"), "frame.id", 100)?,
        captured_at: bounded(
            frame
                .get("capturedAt")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            100,
        ),
        window_id: window_id.clone(),
        pid: frame.get("pid").and_then(Value::as_u64).unwrap_or(0),
        app_name: bounded(
            frame
                .get("appName")
                .and_then(Value::as_str)
                .unwrap_or(&display_name),
            200,
        ),
        window_title: bounded(
            frame
                .get("windowTitle")
                .and_then(Value::as_str)
                .unwrap_or(&display_name),
            500,
        ),
        session_mode: bounded(
            frame
                .get("sessionMode")
                .and_then(Value::as_str)
                .unwrap_or("legacy-display"),
            80,
        ),
        delivery_mode: bounded(
            frame
                .get("deliveryMode")
                .and_then(Value::as_str)
                .unwrap_or("legacy-foreground"),
            80,
        ),
        display_id: required_string(frame.get("displayId"), "frame.displayId", 100)?,
        display_index: frame
            .get("displayIndex")
            .and_then(Value::as_u64)
            .filter(|value| *value < 16)
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::BAD_GATEWAY,
                    "COMPUTER_INVALID_OBSERVATION",
                    "Computer frame displayIndex is invalid",
                )
            })?,
        display_name,
        image_width: bounded_u64("imageWidth", 16_384)?,
        image_height: bounded_u64("imageHeight", 16_384)?,
        screen_x: signed("screenX")?,
        screen_y: signed("screenY")?,
        screen_width: bounded_u64("screenWidth", 100_000)?,
        screen_height: bounded_u64("screenHeight", 100_000)?,
        scale_factor: finite("scaleFactor", 0.1, 16.0)?,
        transport_scale_x: frame
            .get("transportScaleX")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && (0.1..=16.0).contains(value))
            .unwrap_or_else(|| {
                frame
                    .get("scaleFactor")
                    .and_then(Value::as_f64)
                    .unwrap_or(1.0)
            }),
        transport_scale_y: frame
            .get("transportScaleY")
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && (0.1..=16.0).contains(value))
            .unwrap_or_else(|| {
                frame
                    .get("scaleFactor")
                    .and_then(Value::as_f64)
                    .unwrap_or(1.0)
            }),
        rotation: finite("rotation", 0.0, 359.0)?,
        semantic_mode: bounded(
            frame
                .get("semanticMode")
                .and_then(Value::as_str)
                .unwrap_or("unavailable"),
            80,
        ),
        semantic_available: frame
            .get("semanticAvailable")
            .and_then(Value::as_bool)
            .unwrap_or_else(|| !sanitize_computer_elements(frame.get("elements")).is_empty()),
        semantic_error: frame
            .get("semanticError")
            .and_then(Value::as_str)
            .map(|message| bounded(message, 500)),
        pointer: sanitize_computer_pointer(frame.get("pointer"), &window_id)?,
        elements: sanitize_computer_elements(frame.get("elements")),
        share: sanitize_share_status(frame.get("share")),
    })
}

fn sanitize_computer_pointer(
    value: Option<&Value>,
    expected_window_id: &str,
) -> Result<ComputerPointer, ApiError> {
    let pointer = value.and_then(Value::as_object).ok_or_else(|| {
        ApiError::new(
            StatusCode::BAD_GATEWAY,
            "COMPUTER_INVALID_OBSERVATION",
            "Computer frame pointer metadata is missing",
        )
    })?;
    let finite = |name: &str| {
        pointer
            .get(name)
            .and_then(Value::as_f64)
            .filter(|value| value.is_finite() && (-100_000.0..=100_000.0).contains(value))
    };
    let signed = |name: &str| {
        pointer
            .get(name)
            .and_then(Value::as_i64)
            .filter(|value| (-100_000..=100_000).contains(value))
    };
    let window_id = pointer
        .get("windowId")
        .and_then(Value::as_str)
        .filter(|window_id| *window_id == expected_window_id)
        .map(str::to_owned);
    let visible = pointer
        .get("visible")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let image_x = finite("imageX");
    let image_y = finite("imageY");
    if visible && (window_id.is_none() || image_x.is_none() || image_y.is_none()) {
        return Err(ApiError::new(
            StatusCode::BAD_GATEWAY,
            "COMPUTER_INVALID_OBSERVATION",
            "Visible computer pointer metadata is incomplete or targets another window",
        ));
    }
    let style = pointer
        .get("style")
        .and_then(Value::as_object)
        .map(|style| {
            json!({
                "theme": bounded(style.get("theme").and_then(Value::as_str).unwrap_or("unknown"), 80),
                "fill": bounded(style.get("fill").and_then(Value::as_str).unwrap_or("#26C6FF"), 16),
                "outline": bounded(style.get("outline").and_then(Value::as_str).unwrap_or("#FFFFFF"), 16),
                "logicalSize": style.get("logicalSize").and_then(Value::as_u64).unwrap_or(42).clamp(8, 128),
                "hotspot": bounded(style.get("hotspot").and_then(Value::as_str).unwrap_or("tip"), 20),
            })
        })
        .unwrap_or_else(|| json!({}));
    Ok(ComputerPointer {
        id: required_string(pointer.get("id"), "frame.pointer.id", 100)?,
        visible,
        window_id,
        image_x,
        image_y,
        screen_x: signed("screenX"),
        screen_y: signed("screenY"),
        heading_degrees: finite("headingDegrees").unwrap_or(0.0),
        action: bounded(
            pointer
                .get("action")
                .and_then(Value::as_str)
                .unwrap_or("idle"),
            40,
        ),
        pressed: pointer
            .get("pressed")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        sequence: pointer.get("sequence").and_then(Value::as_u64).unwrap_or(0),
        revision: pointer
            .get("revision")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| pointer.get("sequence").and_then(Value::as_u64).unwrap_or(0)),
        buttons_mask: pointer
            .get("buttonsMask")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(31) as u8,
        updated_at: bounded(
            pointer
                .get("updatedAt")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            100,
        ),
        coordinate_space: bounded(
            pointer
                .get("coordinateSpace")
                .and_then(Value::as_str)
                .unwrap_or("image-pixels"),
            40,
        ),
        style,
    })
}

fn sanitize_computer_elements(value: Option<&Value>) -> Vec<ComputerElement> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(500)
                .filter_map(|item| {
                    let item = item.as_object()?;
                    let reference = item.get("ref")?.as_str()?;
                    let role = item.get("role")?.as_str()?;
                    let name = item.get("name")?.as_str()?;
                    if reference.len() > 40 || role.len() > 120 || name.len() > 500 {
                        return None;
                    }
                    let actions: Vec<String> = item
                        .get("actions")
                        .and_then(Value::as_array)
                        .map(|actions| {
                            actions
                                .iter()
                                .take(16)
                                .filter_map(Value::as_str)
                                .filter(|action| action.len() <= 40)
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default();
                    let sensitive = item
                        .get("sensitive")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    let value_redacted = sensitive
                        || item
                            .get("valueRedacted")
                            .and_then(Value::as_bool)
                            .unwrap_or(false);
                    let actions = actions
                        .into_iter()
                        .filter(|action| !(value_redacted && action == "setValue"))
                        .collect();
                    let bounds = item
                        .get("bounds")
                        .and_then(Value::as_object)
                        .and_then(|bounds| {
                            let finite = |name: &str| {
                                bounds.get(name)?.as_f64().filter(|value| {
                                    value.is_finite() && (-100_000.0..=100_000.0).contains(value)
                                })
                            };
                            Some(ComputerElementBounds {
                                x: finite("x")?,
                                y: finite("y")?,
                                width: finite("width")?.clamp(0.0, 100_000.0),
                                height: finite("height")?.clamp(0.0, 100_000.0),
                            })
                        });
                    let screen_bounds = item
                        .get("screenBounds")
                        .and_then(Value::as_object)
                        .and_then(|bounds| {
                            let finite = |name: &str| {
                                bounds.get(name)?.as_f64().filter(|value| {
                                    value.is_finite() && (-100_000.0..=100_000.0).contains(value)
                                })
                            };
                            Some(ComputerElementBounds {
                                x: finite("x")?,
                                y: finite("y")?,
                                width: finite("width")?.clamp(0.0, 100_000.0),
                                height: finite("height")?.clamp(0.0, 100_000.0),
                            })
                        });
                    Some(ComputerElement {
                        reference: reference.to_owned(),
                        role: role.to_owned(),
                        name: name.to_owned(),
                        value: if value_redacted {
                            None
                        } else {
                            item.get("value")
                                .and_then(Value::as_str)
                                .map(|value| bounded(value, 2_000))
                        },
                        sensitive,
                        value_redacted,
                        enabled: item.get("enabled").and_then(Value::as_bool),
                        actions,
                        bounds,
                        coordinate_space: bounded(
                            item.get("coordinateSpace")
                                .and_then(Value::as_str)
                                .unwrap_or("image-pixels"),
                            40,
                        ),
                        screen_bounds,
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn sanitize_share_status(value: Option<&Value>) -> Value {
    let Some(share) = value.and_then(Value::as_object) else {
        return json!({ "active": false });
    };
    let active = share
        .get("active")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !active {
        return json!({
            "active": false,
            "stopped": share.get("stopped").and_then(Value::as_bool).unwrap_or(false),
            "reason": bounded(share.get("reason").and_then(Value::as_str).unwrap_or("inactive"), 40),
        });
    }
    json!({
        "active": true,
        "id": bounded(share.get("id").and_then(Value::as_str).unwrap_or("unknown"), 100),
        "windowId": bounded(share.get("windowId").and_then(Value::as_str).unwrap_or("unknown"), 100),
        "fps": share.get("fps").and_then(Value::as_u64).unwrap_or(1).clamp(1, 10),
        "sequence": share.get("sequence").and_then(Value::as_u64).unwrap_or(0),
        "startedAt": bounded(share.get("startedAt").and_then(Value::as_str).unwrap_or("unknown"), 100),
        "captureScope": "exact-window",
        "cursorComposited": share.get("cursorComposited").and_then(Value::as_bool).unwrap_or(true),
        "droppedFrames": share.get("droppedFrames").and_then(Value::as_u64).unwrap_or(0),
        "ackPaced": share.get("ackPaced").and_then(Value::as_bool).unwrap_or(false),
        "lastAckedSequence": share.get("lastAckedSequence").and_then(Value::as_u64).unwrap_or(0),
        "backpressure": bounded(share.get("backpressure").and_then(Value::as_str).unwrap_or("producer-blocking"), 40),
    })
}

fn sanitize_computer_windows(value: Option<&Value>) -> Vec<ComputerWindow> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let item = item.as_object()?;
                    let id = item.get("id")?.as_str()?;
                    let pid = item.get("pid")?.as_u64()?;
                    let width = item.get("width")?.as_u64()?;
                    let height = item.get("height")?.as_u64()?;
                    if id.len() > 100
                        || pid == 0
                        || width == 0
                        || width > 100_000
                        || height == 0
                        || height > 100_000
                    {
                        return None;
                    }
                    Some(ComputerWindow {
                        id: id.to_owned(),
                        pid,
                        app_name: bounded(
                            item.get("appName").and_then(Value::as_str).unwrap_or("App"),
                            200,
                        ),
                        title: bounded(
                            item.get("title")
                                .and_then(Value::as_str)
                                .unwrap_or("Window"),
                            500,
                        ),
                        width,
                        height,
                        focused: item
                            .get("focused")
                            .and_then(Value::as_bool)
                            .unwrap_or(false),
                    })
                })
                .take(128)
                .collect()
        })
        .unwrap_or_default()
}

fn sanitize_params(
    method: &str,
    input: Value,
    target_tab_id: Option<u64>,
    viewport: Option<(f64, f64)>,
) -> Result<Value, ApiError> {
    let source = input.as_object().cloned().unwrap_or_default();
    let implied_tab_id = match source.get("tabId") {
        Some(value) if !value.is_null() => Some(as_u64(value, "tabId")?),
        _ => target_tab_id,
    };
    let with_tab = || {
        implied_tab_id
            .map(|tab_id| json!({ "tabId": tab_id }))
            .ok_or_else(|| ApiError::bad_request("Select a target tab first"))
    };

    let mut sanitized = match method {
        "status" | "browser.control.status" | "tabs.list" | "tabs.new" => Ok(json!({})),
        "browser.control.start" => {
            let mut output = object(with_tab()?);
            let ttl_ms = source
                .get("ttlMs")
                .map(|value| as_u64(value, "ttlMs"))
                .transpose()?
                .unwrap_or(300_000);
            if !(15_000..=900_000).contains(&ttl_ms) {
                return Err(ApiError::bad_request(
                    "ttlMs must be between 15000 and 900000",
                ));
            }
            output.insert("ttlMs".to_owned(), json!(ttl_ms));
            Ok(Value::Object(output))
        }
        "browser.control.stop" => {
            let mut output = Map::new();
            if let Some(session_id) = source.get("sessionId") {
                output.insert(
                    "sessionId".to_owned(),
                    Value::String(required_string(Some(session_id), "sessionId", 100)?),
                );
            }
            Ok(Value::Object(output))
        }
        "tabs.activate" | "tabs.close" => Ok(json!({
            "tabId": as_u64(source.get("tabId").unwrap_or(&Value::Null), "tabId")?
        })),
        "page.observe" | "page.back" | "page.forward" | "page.reload" => with_tab(),
        "page.navigate" => {
            let mut output = object(with_tab()?);
            output.insert(
                "url".to_owned(),
                Value::String(required_string(source.get("url"), "url", 4_096)?),
            );
            Ok(Value::Object(output))
        }
        "page.click" => click_params(&source, with_tab()?),
        "page.hover" => ref_params(&source, with_tab()?, None),
        "page.fill" => ref_params(&source, with_tab()?, Some(("text", 10_000))),
        "page.select" => ref_params(&source, with_tab()?, Some(("value", 1_000))),
        "page.key" => key_params(&source, with_tab()?),
        "page.waitFor" => {
            let mut output = object(with_tab()?);
            let mut condition_count = 0;
            for (name, max) in [("text", 500), ("textGone", 500), ("urlPrefix", 4_096)] {
                let Some(value) = source.get(name).filter(|value| !value.is_null()) else {
                    continue;
                };
                let value = required_string(Some(value), name, max)?;
                if value.is_empty() {
                    return Err(ApiError::bad_request(format!("{name} must not be empty")));
                }
                output.insert(name.to_owned(), Value::String(value));
                condition_count += 1;
            }
            if let Some(value) = source
                .get("mutationQuietMs")
                .filter(|value| !value.is_null())
            {
                let quiet_ms = as_u64(value, "mutationQuietMs")?;
                if !(250..=5_000).contains(&quiet_ms) {
                    return Err(ApiError::bad_request(
                        "mutationQuietMs must be between 250 and 5000",
                    ));
                }
                output.insert("mutationQuietMs".to_owned(), json!(quiet_ms));
                condition_count += 1;
            }
            if condition_count == 0 {
                return Err(ApiError::bad_request(
                    "page.waitFor needs at least one condition: text, textGone, urlPrefix, or mutationQuietMs",
                ));
            }
            let timeout_ms = source
                .get("timeoutMs")
                .map(|value| as_u64(value, "timeoutMs"))
                .transpose()?
                .unwrap_or(5_000)
                .clamp(100, 12_000);
            output.insert("timeoutMs".to_owned(), json!(timeout_ms));
            Ok(Value::Object(output))
        }
        "page.scroll" => scroll_params(&source, with_tab()?),
        "page.batch" => {
            let base = with_tab()?;
            let generation = required_string(source.get("generation"), "generation", 100)?;
            let actions = source
                .get("actions")
                .and_then(Value::as_array)
                .ok_or_else(|| ApiError::bad_request("actions must be an array of sub-actions"))?;
            if actions.is_empty() || actions.len() > BATCH_MAX_ACTIONS {
                return Err(ApiError::bad_request(format!(
                    "page.batch needs between 1 and {BATCH_MAX_ACTIONS} sub-actions"
                )));
            }
            let batch_tab_id = base.get("tabId").and_then(Value::as_u64);
            let mut sanitized_actions = Vec::with_capacity(actions.len());
            for (index, action) in actions.iter().enumerate() {
                let action = action.as_object().ok_or_else(|| {
                    ApiError::bad_request(format!(
                        "actions[{index}] must be an object with a method"
                    ))
                })?;
                let sub_method = action.get("method").and_then(Value::as_str).unwrap_or("");
                if !BATCH_SUB_METHODS.contains(&sub_method) {
                    return Err(ApiError::bad_request(format!(
                        "actions[{index}] method {} is not batchable; allowed sub-methods are page.click, page.fill, page.select, page.key, and page.scroll",
                        bounded(sub_method, 80)
                    )));
                }
                // The batch tabId and generation are authoritative: every
                // sub-action is proven against the same snapshot epoch.
                if let Some(value) = action.get("tabId").filter(|value| !value.is_null())
                    && Some(as_u64(value, "tabId")?) != batch_tab_id
                {
                    return Err(ApiError::bad_request(format!(
                        "actions[{index}] tabId must match the batch tabId"
                    )));
                }
                if let Some(value) = action.get("generation").filter(|value| !value.is_null())
                    && required_string(Some(value), "generation", 100)? != generation
                {
                    return Err(ApiError::bad_request(format!(
                        "actions[{index}] generation must match the batch generation"
                    )));
                }
                let mut sub_source = action.clone();
                sub_source.insert("generation".to_owned(), Value::String(generation.clone()));
                // Lease bindings are injected exactly once at the batch top
                // level, never per step.
                for name in ["controlSessionId", "turn", "moveSequence"] {
                    sub_source.remove(name);
                }
                let sanitized = match sub_method {
                    "page.click" => click_params(&sub_source, base.clone())?,
                    "page.fill" => ref_params(&sub_source, base.clone(), Some(("text", 10_000)))?,
                    "page.select" => ref_params(&sub_source, base.clone(), Some(("value", 1_000)))?,
                    "page.key" => key_params(&sub_source, base.clone())?,
                    "page.scroll" => scroll_params(&sub_source, base.clone())?,
                    _ => {
                        return Err(ApiError::bad_request(format!(
                            "actions[{index}] method is not batchable"
                        )));
                    }
                };
                let mut sanitized = object(sanitized);
                sanitized.insert("method".to_owned(), json!(sub_method));
                sanitized_actions.push(Value::Object(sanitized));
            }
            let mut output = object(base);
            output.insert("generation".to_owned(), Value::String(generation));
            output.insert("actions".to_owned(), Value::Array(sanitized_actions));
            Ok(Value::Object(output))
        }
        "page.handleDialog" => {
            let mut output = object(with_tab()?);
            let accept = source
                .get("accept")
                .and_then(Value::as_bool)
                .ok_or_else(|| ApiError::bad_request("accept must be true or false"))?;
            output.insert("accept".to_owned(), json!(accept));
            if let Some(value) = source.get("promptText").filter(|value| !value.is_null()) {
                output.insert(
                    "promptText".to_owned(),
                    Value::String(required_string(Some(value), "promptText", 1_000)?),
                );
            }
            Ok(Value::Object(output))
        }
        "page.clickAt" => {
            let mut output = object(with_tab()?);
            output.insert(
                "generation".to_owned(),
                Value::String(required_string(
                    source.get("generation"),
                    "generation",
                    100,
                )?),
            );
            let x = finite_number(source.get("x"), f64::NAN, "x")?;
            let y = finite_number(source.get("y"), f64::NAN, "y")?;
            if !(0.0..=100_000.0).contains(&x) || !(0.0..=100_000.0).contains(&y) {
                return Err(ApiError::bad_request(
                    "x and y must be between 0 and 100000",
                ));
            }
            let extents = normalized_extents(
                &source,
                "viewport",
                viewport,
                (
                    "NO_BROWSER_OBSERVATION",
                    "Observe the page first; normalized1000 coordinates need a current viewport observation",
                ),
            )?;
            let x = scaled_coordinate(x, "x", extents.map(|(width, _)| width))?;
            let y = scaled_coordinate(y, "y", extents.map(|(_, height)| height))?;
            let button = optional_string(source.get("button"), "left", "button", 8)?;
            if !["left", "middle", "right"].contains(&button.as_str()) {
                return Err(ApiError::bad_request(
                    "button must be left, middle, or right",
                ));
            }
            let click_count = source
                .get("clickCount")
                .map(|value| as_u64(value, "clickCount"))
                .transpose()?
                .unwrap_or(1);
            if !(1..=3).contains(&click_count) {
                return Err(ApiError::bad_request("clickCount must be between 1 and 3"));
            }
            output.extend([
                ("x".to_owned(), json!(x)),
                ("y".to_owned(), json!(y)),
                ("button".to_owned(), json!(button)),
                ("clickCount".to_owned(), json!(click_count)),
            ]);
            Ok(Value::Object(output))
        }
        "page.typeText" => {
            let mut output = object(with_tab()?);
            output.insert(
                "generation".to_owned(),
                Value::String(required_string(
                    source.get("generation"),
                    "generation",
                    100,
                )?),
            );
            output.insert(
                "text".to_owned(),
                Value::String(required_string(source.get("text"), "text", 100_000)?),
            );
            Ok(Value::Object(output))
        }
        "page.evaluate" => {
            let mut output = object(with_tab()?);
            output.insert(
                "expression".to_owned(),
                Value::String(required_string(
                    source.get("expression"),
                    "expression",
                    100_000,
                )?),
            );
            Ok(Value::Object(output))
        }
        _ => Err(ApiError::bad_request("Unsupported action")),
    }?;

    if method.starts_with("page.") && !matches!(method, "page.observe" | "page.waitFor") {
        let output = sanitized.as_object_mut().ok_or_else(|| {
            ApiError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "INVALID_SANITIZER_STATE",
                "Browser action sanitizer did not return an object",
            )
        })?;
        if let Some(value) = source.get("controlSessionId") {
            output.insert(
                "controlSessionId".to_owned(),
                Value::String(required_string(Some(value), "controlSessionId", 100)?),
            );
        }
        for name in ["turn", "moveSequence"] {
            if let Some(value) = source.get(name) {
                let value = as_u64(value, name)?;
                if value > 9_007_199_254_740_991 {
                    return Err(ApiError::bad_request(format!(
                        "{name} exceeds the JavaScript safe-integer range"
                    )));
                }
                output.insert(name.to_owned(), json!(value));
            }
        }
    }
    Ok(sanitized)
}

/// Binds the action to the browser-control lease. `bind_freshness` also binds
/// the observation `turn` and the pointer `moveSequence`, which force the
/// caller to observe before acting; it is dropped only for `page.handleDialog`
/// — see the call site.
fn bind_browser_control(params: &mut Value, control: &Value, bind_freshness: bool) {
    if control.get("active").and_then(Value::as_bool) != Some(true) {
        return;
    }
    let Some(output) = params.as_object_mut() else {
        return;
    };
    if !output.contains_key("controlSessionId")
        && let Some(session_id) = control.get("sessionId").and_then(Value::as_str)
        && !session_id.is_empty()
    {
        output.insert(
            "controlSessionId".to_owned(),
            Value::String(session_id.to_owned()),
        );
    }
    if !bind_freshness {
        return;
    }
    for name in ["turn", "moveSequence"] {
        if !output.contains_key(name)
            && let Some(value) = control.get(name).and_then(Value::as_u64)
        {
            output.insert(name.to_owned(), json!(value));
        }
    }
}

/// Accepts exactly the three published element-reference formats: a legacy
/// bare ref such as `e12`, an epoch-embedded top-frame ref such as
/// `<generation>.e12`, and a frame-scoped ref such as `<generation>.f2.e12`.
/// A two-segment ref is always `<generation>.<element>`, never
/// `<frame>.<element>`, so the grammar stays unambiguous:
/// `^([a-z0-9-]{1,64}\.((f([1-9]|1[0-6]))\.)?)?e[1-9][0-9]{0,3}$`.
fn valid_element_ref(reference: &str) -> bool {
    let parts: Vec<&str> = reference.split('.').collect();
    match parts.as_slice() {
        [element] => valid_element_key(element),
        [generation, element] => valid_generation(generation) && valid_element_key(element),
        [generation, frame, element] => {
            valid_generation(generation) && valid_frame_key(frame) && valid_element_key(element)
        }
        _ => false,
    }
}

fn valid_generation(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'))
}

/// `f1` through `f16`, no leading zero: the extension can never mint a frame
/// key outside its own attachment cap, so anything else is malformed input.
fn valid_frame_key(value: &str) -> bool {
    let Some(digits) = value.strip_prefix('f') else {
        return false;
    };
    if digits.is_empty()
        || digits.len() > 2
        || digits.starts_with('0')
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return false;
    }
    digits
        .parse::<u32>()
        .is_ok_and(|index| (1..=MAX_FRAME_INDEX).contains(&index))
}

fn valid_element_key(value: &str) -> bool {
    let Some(digits) = value.strip_prefix('e') else {
        return false;
    };
    !digits.is_empty()
        && digits.len() <= 4
        && !digits.starts_with('0')
        && digits.bytes().all(|byte| byte.is_ascii_digit())
}

/// Sanitizes the optional `modifiers` array on `page.click` into a
/// deduplicated subset of the four supported modifier names.
fn sanitize_click_modifiers(value: Option<&Value>) -> Result<Vec<String>, ApiError> {
    let items = match value {
        None | Some(Value::Null) => return Ok(Vec::new()),
        Some(Value::Array(items)) => items,
        Some(_) => {
            return Err(ApiError::bad_request(
                "modifiers must be an array of modifier names",
            ));
        }
    };
    if items.len() > 4 {
        return Err(ApiError::bad_request(
            "modifiers may list each of Shift, Control, Alt, and Meta at most once",
        ));
    }
    let mut modifiers: Vec<String> = Vec::new();
    for item in items {
        let name = item
            .as_str()
            .ok_or_else(|| ApiError::bad_request("modifiers must be an array of modifier names"))?;
        if !["Shift", "Control", "Alt", "Meta"].contains(&name) {
            return Err(ApiError::bad_request(
                "modifiers may only contain Shift, Control, Alt, or Meta",
            ));
        }
        if !modifiers.iter().any(|existing| existing == name) {
            modifiers.push(name.to_owned());
        }
    }
    Ok(modifiers)
}

/// Canonical named keys shared with the extension's key-chord parser.
const CANONICAL_NAMED_KEYS: &[&str] = &[
    "Tab",
    "Enter",
    "Escape",
    "Backspace",
    "ArrowLeft",
    "ArrowUp",
    "ArrowRight",
    "ArrowDown",
    "PageUp",
    "PageDown",
    "End",
    "Home",
    "Space",
    "Delete",
    "Insert",
    "ContextMenu",
    "CapsLock",
    "PrintScreen",
    "Pause",
];

/// Normalizes one key or chord into the canonical grammar before relay, so
/// the extension and the computer helper only ever see canonical chords.
/// Accepts the canonical dialect ("Meta+L") and the lowercase vendor dialect
/// ("ctrl+shift+t", "cmd+l"); rejects unknown tokens and more than three
/// modifiers with a clear error.
fn normalize_key_chord(chord: &str) -> Result<String, ApiError> {
    let parts: Vec<&str> = chord.split('+').map(str::trim).collect();
    if parts.is_empty() || parts.iter().any(|part| part.is_empty()) {
        return Err(ApiError::bad_request(
            "key must be a key or chord such as Enter, Meta+L, or ctrl+shift+t",
        ));
    }
    let (&key, modifiers) = parts.split_last().unwrap();
    if modifiers.len() > 3 {
        return Err(ApiError::bad_request(
            "key chords support at most 3 modifiers",
        ));
    }
    // Ranked in the CDP modifier bitmask order so equivalent chords always
    // normalize to one canonical spelling.
    let mut ranked: Vec<(usize, &'static str)> = Vec::new();
    for modifier in modifiers {
        let (rank, canonical) = match modifier.to_ascii_lowercase().as_str() {
            "alt" | "option" | "opt" => (0, "Alt"),
            "control" | "ctrl" => (1, "Control"),
            "meta" | "cmd" | "command" | "win" | "super" => (2, "Meta"),
            "shift" => (3, "Shift"),
            other => {
                return Err(ApiError::bad_request(format!(
                    "unknown key modifier {other}; use Control, Alt, Shift, or Meta (aliases: ctrl, cmd, option, opt, win, super)"
                )));
            }
        };
        if !ranked.contains(&(rank, canonical)) {
            ranked.push((rank, canonical));
        }
    }
    ranked.sort_unstable();
    let mut normalized: Vec<String> = ranked
        .into_iter()
        .map(|(_, canonical)| canonical.to_owned())
        .collect();
    let in_chord = !normalized.is_empty();
    normalized.push(normalize_key_name(key, in_chord)?);
    Ok(normalized.join("+"))
}

/// Normalizes one key token: canonical names pass through, lowercase vendor
/// aliases map onto them, and any other single printable character stays
/// itself. A bare single letter keeps its case verbatim (legacy v0.9 clients
/// send `j` for Gmail-style shortcuts, and uppercasing would imply Shift);
/// letters inside modifier chords canonicalize to uppercase, matching the
/// documented `Meta+L` form.
fn normalize_key_name(key: &str, in_chord: bool) -> Result<String, ApiError> {
    if CANONICAL_NAMED_KEYS.contains(&key) {
        return Ok(key.to_owned());
    }
    let function_number = |token: &str| {
        token
            .parse::<u8>()
            .ok()
            .filter(|number| (1..=12).contains(number))
    };
    if let Some(number) = key.strip_prefix('F').and_then(function_number) {
        return Ok(format!("F{number}"));
    }
    let lowered = key.to_ascii_lowercase();
    let alias = match lowered.as_str() {
        "esc" | "escape" => Some("Escape"),
        "return" | "enter" => Some("Enter"),
        "del" | "delete" => Some("Delete"),
        "space" | "spacebar" => Some("Space"),
        "tab" => Some("Tab"),
        "backspace" => Some("Backspace"),
        "up" | "arrowup" => Some("ArrowUp"),
        "down" | "arrowdown" => Some("ArrowDown"),
        "left" | "arrowleft" => Some("ArrowLeft"),
        "right" | "arrowright" => Some("ArrowRight"),
        "pageup" => Some("PageUp"),
        "pagedown" => Some("PageDown"),
        "home" => Some("Home"),
        "end" => Some("End"),
        "insert" => Some("Insert"),
        "contextmenu" => Some("ContextMenu"),
        "capslock" => Some("CapsLock"),
        "printscreen" => Some("PrintScreen"),
        "pause" => Some("Pause"),
        _ => None,
    };
    if let Some(alias) = alias {
        return Ok(alias.to_owned());
    }
    if let Some(number) = lowered.strip_prefix('f').and_then(function_number) {
        return Ok(format!("F{number}"));
    }
    let mut characters = key.chars();
    if let (Some(character), None) = (characters.next(), characters.next()) {
        if character.is_ascii_alphabetic() {
            return Ok(if in_chord {
                character.to_ascii_uppercase().to_string()
            } else {
                character.to_string()
            });
        }
        if !character.is_control() && !character.is_whitespace() {
            return Ok(character.to_string());
        }
    }
    Err(ApiError::bad_request(format!(
        "unknown key token {key}; use a named key such as Enter or Escape, F1-F12, or a single character"
    )))
}

/// The `page.click` parameter core: a validated element ref plus the pointer
/// verb options. Shared by the top-level command and `page.batch` sub-actions.
fn click_params(source: &Map<String, Value>, base: Value) -> Result<Value, ApiError> {
    let mut output = object(ref_params(source, base, None)?);
    let button = optional_string(source.get("button"), "left", "button", 8)?;
    if !["left", "middle", "right"].contains(&button.as_str()) {
        return Err(ApiError::bad_request(
            "button must be left, middle, or right",
        ));
    }
    let click_count = source
        .get("clickCount")
        .map(|value| as_u64(value, "clickCount"))
        .transpose()?
        .unwrap_or(1);
    if !(1..=3).contains(&click_count) {
        return Err(ApiError::bad_request("clickCount must be between 1 and 3"));
    }
    output.extend([
        ("button".to_owned(), json!(button)),
        ("clickCount".to_owned(), json!(click_count)),
        (
            "modifiers".to_owned(),
            json!(sanitize_click_modifiers(source.get("modifiers"))?),
        ),
    ]);
    Ok(Value::Object(output))
}

/// The `page.key` parameter core: a generation binding plus one normalized
/// key chord. Shared by the top-level command and `page.batch` sub-actions.
fn key_params(source: &Map<String, Value>, base: Value) -> Result<Value, ApiError> {
    let mut output = object(base);
    output.insert(
        "generation".to_owned(),
        Value::String(required_string(
            source.get("generation"),
            "generation",
            100,
        )?),
    );
    let key = required_string(source.get("key"), "key", 80)?;
    output.insert("key".to_owned(), Value::String(normalize_key_chord(&key)?));
    Ok(Value::Object(output))
}

/// The `page.scroll` parameter core: a generation binding plus clamped wheel
/// deltas. Shared by the top-level command and `page.batch` sub-actions.
fn scroll_params(source: &Map<String, Value>, base: Value) -> Result<Value, ApiError> {
    let mut output = object(base);
    output.insert(
        "generation".to_owned(),
        Value::String(required_string(
            source.get("generation"),
            "generation",
            100,
        )?),
    );
    output.insert(
        "deltaX".to_owned(),
        json!(
            finite_number(source.get("deltaX"), 0.0, "deltaX")?
                .clamp(-5_000.0, 5_000.0)
                .trunc()
        ),
    );
    output.insert(
        "deltaY".to_owned(),
        json!(
            finite_number(source.get("deltaY"), 0.0, "deltaY")?
                .clamp(-5_000.0, 5_000.0)
                .trunc()
        ),
    );
    Ok(Value::Object(output))
}

fn ref_params(
    source: &Map<String, Value>,
    base: Value,
    extra: Option<(&str, usize)>,
) -> Result<Value, ApiError> {
    let mut output = object(base);
    let reference = required_string(source.get("ref"), "ref", 80)?;
    if !valid_element_ref(&reference) {
        return Err(ApiError::bad_request(
            "ref must be an element reference such as e12, <generation>.e12, or <generation>.f2.e12",
        ));
    }
    output.insert("ref".to_owned(), Value::String(reference));
    output.insert(
        "generation".to_owned(),
        Value::String(required_string(
            source.get("generation"),
            "generation",
            100,
        )?),
    );
    if let Some((name, max)) = extra {
        output.insert(
            name.to_owned(),
            Value::String(required_string(source.get(name), name, max)?),
        );
    }
    Ok(Value::Object(output))
}

fn parse_json_body(bytes: &[u8]) -> Result<Value, ApiError> {
    if bytes.len() > MAX_BODY_BYTES {
        return Err(ApiError::new(
            StatusCode::PAYLOAD_TOO_LARGE,
            "BODY_TOO_LARGE",
            "Request body is too large",
        ));
    }
    serde_json::from_slice(bytes)
        .map_err(|_| ApiError::bad_request("Request body must be valid JSON"))
}

fn required_string(value: Option<&Value>, name: &str, max: usize) -> Result<String, ApiError> {
    let value = value
        .and_then(Value::as_str)
        .ok_or_else(|| ApiError::bad_request(format!("{name} must be a string")))?;
    if value.chars().count() > max {
        return Err(ApiError::bad_request(format!("{name} is too long")));
    }
    Ok(value.to_owned())
}

fn optional_string(
    value: Option<&Value>,
    default: &str,
    name: &str,
    max: usize,
) -> Result<String, ApiError> {
    match value {
        None | Some(Value::Null) => Ok(default.to_owned()),
        other => required_string(other, name, max),
    }
}

fn as_u64(value: &Value, name: &str) -> Result<u64, ApiError> {
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse().ok())
        .ok_or_else(|| ApiError::bad_request(format!("{name} must be a non-negative integer")))
}

fn finite_number(value: Option<&Value>, default: f64, name: &str) -> Result<f64, ApiError> {
    let number = match value {
        None | Some(Value::Null) => default,
        Some(value) => value
            .as_f64()
            .or_else(|| value.as_str()?.parse().ok())
            .ok_or_else(|| ApiError::bad_request(format!("{name} must be a number")))?,
    };
    if !number.is_finite() {
        return Err(ApiError::bad_request(format!(
            "{name} must be a finite number"
        )));
    }
    Ok(number)
}

fn object(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

fn sanitize_browser_control(value: &Value) -> Value {
    let human_paused = value
        .get("humanPaused")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let human_pause = sanitize_human_control_pause(value.get("humanPause").unwrap_or(&Value::Null));
    let revocation_pending = value
        .get("revocationPending")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if value.get("active").and_then(Value::as_bool) != Some(true) {
        return json!({
            "active": false,
            "humanPaused": human_paused,
            "humanPause": human_pause,
            "revocationPending": revocation_pending,
            "revocation": sanitize_control_revocation(
                value.get("revocation").unwrap_or(&Value::Null)
            )
        });
    }
    let cursor = value.get("cursor").unwrap_or(&Value::Null);
    json!({
        "active": true,
        "humanPaused": human_paused,
        "humanPause": human_pause,
        "revocationPending": revocation_pending,
        "sessionId": bounded(value.get("sessionId").and_then(Value::as_str).unwrap_or(""), 100),
        "tabId": value.get("tabId").and_then(Value::as_u64),
        "startedAt": value.get("startedAt").and_then(Value::as_u64),
        "expiresAt": value.get("expiresAt").and_then(Value::as_u64),
        "lastHeartbeatAt": value.get("lastHeartbeatAt").and_then(Value::as_u64),
        "turn": value.get("turn").and_then(Value::as_u64).unwrap_or(0),
        "moveSequence": value.get("moveSequence").and_then(Value::as_u64).unwrap_or(0),
        "cursor": {
            "x": cursor.get("x").and_then(Value::as_f64),
            "y": cursor.get("y").and_then(Value::as_f64),
            "visible": cursor.get("visible").and_then(Value::as_bool).unwrap_or(false),
            "updatedAt": cursor.get("updatedAt").and_then(Value::as_u64)
        }
    })
}

fn sanitize_human_control_pause(value: &Value) -> Value {
    if !value.is_object() {
        return Value::Null;
    }
    json!({
        "paused": value.get("paused").and_then(Value::as_bool).unwrap_or(true),
        "reason": bounded(value.get("reason").and_then(Value::as_str).unwrap_or("released_by_user"), 100),
        "at": value.get("at").and_then(Value::as_u64),
        "tabId": value.get("tabId").and_then(Value::as_u64)
    })
}

fn sanitize_control_revocation(value: &Value) -> Value {
    if !value.is_object() {
        return Value::Null;
    }
    json!({
        "tabId": value.get("tabId").and_then(Value::as_u64),
        "sessionId": bounded(value.get("sessionId").and_then(Value::as_str).unwrap_or(""), 100),
        "reason": bounded(value.get("reason").and_then(Value::as_str).unwrap_or("unknown"), 100),
        "at": value.get("at").and_then(Value::as_u64),
        "requiresExplicitStart": value.get("requiresExplicitStart").and_then(Value::as_bool).unwrap_or(false)
    })
}

fn sanitize_tab(value: &Value) -> Option<TabInfo> {
    Some(TabInfo {
        id: value.get("id")?.as_u64()?,
        title: bounded(
            value.get("title").and_then(Value::as_str).unwrap_or(""),
            300,
        ),
        url: bounded(
            value.get("url").and_then(Value::as_str).unwrap_or(""),
            4_096,
        ),
        active: value
            .get("active")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// One observation's published element list, plus whether the cap dropped
/// anything. The cap is applied per class, not flat: the extension already
/// reserves a fifth of its own 500-slot merge budget for frame elements and
/// appends them AFTER the top document's, so a flat `take` would publish
/// nothing but top elements on any page with 250 of them — while `frames`
/// and `frameSummary` still advertised merged frames whose `<gen>.f<k>.e<n>`
/// refs appeared nowhere in `elements`. The same 4:1 reservation is applied
/// here, and whichever class runs out first hands its unused slots to the
/// other, so the cap is always filled.
fn publish_elements(raw_elements: &[Value]) -> (Vec<ElementInfo>, bool) {
    let sanitized: Vec<ElementInfo> = raw_elements.iter().filter_map(sanitize_element).collect();
    if sanitized.len() <= MAX_PUBLISHED_ELEMENTS {
        return (sanitized, false);
    }
    let total = sanitized.len();
    let frame_total = sanitized
        .iter()
        .filter(|element| element.frame_ref.is_some())
        .count();
    let top_total = total - frame_total;
    let frame_slots = frame_total
        .min(MAX_PUBLISHED_FRAME_ELEMENTS.max(MAX_PUBLISHED_ELEMENTS.saturating_sub(top_total)));
    let top_slots = top_total.min(MAX_PUBLISHED_ELEMENTS - frame_slots);
    let mut frames_taken = 0;
    let mut top_taken = 0;
    let mut published = Vec::with_capacity(MAX_PUBLISHED_ELEMENTS);
    for element in sanitized {
        let (taken, limit) = if element.frame_ref.is_some() {
            (&mut frames_taken, frame_slots)
        } else {
            (&mut top_taken, top_slots)
        };
        if *taken >= limit {
            continue;
        }
        *taken += 1;
        published.push(element);
    }
    let truncated = published.len() < total;
    (published, truncated)
}

/// Restates every frame's `elementCount` as what actually reached `elements`.
/// The extension counts what it merged; the publication cap above may drop
/// some of that, and a frame advertising a count no ref backs is a frame the
/// caller cannot act on.
fn reconcile_frame_counts(frames: Vec<FrameInfo>, published: &[ElementInfo]) -> Vec<FrameInfo> {
    let mut counts: HashMap<&str, u64> = HashMap::new();
    for element in published {
        if let Some(frame_ref) = element.frame_ref.as_deref() {
            *counts.entry(frame_ref).or_default() += 1;
        }
    }
    frames
        .into_iter()
        .map(|mut frame| {
            let published_count = counts
                .get(frame.reference.as_str())
                .copied()
                .unwrap_or_default();
            frame.truncated = frame.truncated || published_count < frame.element_count;
            frame.element_count = published_count;
            frame
        })
        .collect()
}

fn sanitize_element(value: &Value) -> Option<ElementInfo> {
    let reference = bounded(value.get("ref")?.as_str()?, 80);
    if reference.is_empty() {
        return None;
    }
    let frame_reference = value
        .get("frameRef")
        .and_then(Value::as_str)
        .map(|frame_ref| bounded(frame_ref, 8))
        .filter(|frame_ref| valid_frame_key(frame_ref));
    Some(ElementInfo {
        reference,
        role: bounded(value.get("role").and_then(Value::as_str).unwrap_or(""), 80),
        name: bounded(value.get("name").and_then(Value::as_str).unwrap_or(""), 500),
        element_type: bounded(value.get("type").and_then(Value::as_str).unwrap_or(""), 80),
        disabled: value
            .get("disabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        checked: value.get("checked").and_then(Value::as_bool),
        selected: value.get("selected").and_then(Value::as_bool),
        sensitive: value
            .get("sensitive")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        in_viewport: value
            .get("inViewport")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        risk: value
            .get("risk")
            .and_then(Value::as_str)
            .map(|risk| bounded(risk, 200)),
        bounds: value
            .get("bounds")
            .and_then(Value::as_object)
            .map(|bounds| Bounds {
                x: rounded(bounds.get("x")),
                y: rounded(bounds.get("y")),
                width: rounded(bounds.get("width")),
                height: rounded(bounds.get("height")),
            }),
        frame_ref: frame_reference.clone(),
        frame_id: frame_reference.as_ref().map(|_| {
            bounded(
                value.get("frameId").and_then(Value::as_str).unwrap_or(""),
                100,
            )
        }),
        frame_url_origin: frame_reference.as_ref().map(|_| {
            bounded(
                value
                    .get("frameUrlOrigin")
                    .and_then(Value::as_str)
                    .unwrap_or(""),
                300,
            )
        }),
        // Cross-origin provenance requires a frame ref: a payload that claims
        // `crossOrigin` on a top-frame element is claiming something the
        // extension never mints, so the claim is dropped.
        cross_origin: match (
            frame_reference.is_some(),
            value.get("crossOrigin").and_then(Value::as_bool),
        ) {
            (true, Some(flag)) => Some(flag),
            (true, None) => Some(true),
            (false, _) => None,
        },
    })
}

/// One merged cross-origin frame. A frame whose `ref` is outside the
/// published `f1`..`f16` grammar, or that carries no frame id, is dropped
/// rather than published with an unusable reference.
fn sanitize_frame(value: &Value) -> Option<FrameInfo> {
    let reference = bounded(value.get("ref")?.as_str()?, 8);
    if !valid_frame_key(&reference) {
        return None;
    }
    let frame_id = bounded(
        value.get("frameId").and_then(Value::as_str).unwrap_or(""),
        100,
    );
    if frame_id.is_empty() {
        return None;
    }
    let offset = value.get("offset").and_then(Value::as_object);
    let size = value.get("size").and_then(Value::as_object);
    Some(FrameInfo {
        reference,
        frame_id,
        url_origin: bounded(
            value.get("urlOrigin").and_then(Value::as_str).unwrap_or(""),
            300,
        ),
        cross_origin: value
            .get("crossOrigin")
            .and_then(Value::as_bool)
            .unwrap_or(true),
        depth: value
            .get("depth")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .clamp(1, MAX_OBSERVED_FRAME_DEPTH),
        offset: Offset {
            x: rounded(offset.and_then(|item| item.get("x"))),
            y: rounded(offset.and_then(|item| item.get("y"))),
        },
        size: Size {
            width: rounded(size.and_then(|item| item.get("width"))),
            height: rounded(size.and_then(|item| item.get("height"))),
        },
        element_count: value
            .get("elementCount")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(FRAME_SUMMARY_COUNTER_MAX),
        truncated: value
            .get("truncated")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// Bounded honesty report about frames the extension saw but did not merge.
/// Every counter is clamped and the skip list is capped, so a hostile page
/// cannot inflate `/api/state` through its own iframe count.
fn sanitize_frame_summary(value: &Value) -> Value {
    if !value.is_object() {
        return Value::Null;
    }
    let counter = |name: &str| {
        value
            .get(name)
            .and_then(Value::as_u64)
            .unwrap_or(0)
            .min(FRAME_SUMMARY_COUNTER_MAX)
    };
    let skipped = value
        .get("skipped")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .take(MAX_OBSERVED_FRAME_SKIPS)
                .map(|item| {
                    json!({
                        "urlOrigin": bounded(item.get("urlOrigin").and_then(Value::as_str).unwrap_or(""), 300),
                        "reason": bounded(item.get("reason").and_then(Value::as_str).unwrap_or("unknown"), 40)
                    })
                })
                .collect::<Vec<Value>>()
        })
        .unwrap_or_default();
    json!({
        "supported": value.get("supported").and_then(Value::as_bool).unwrap_or(false),
        "mode": bounded(value.get("mode").and_then(Value::as_str).unwrap_or("cdp-auto-attach"), 40),
        "reason": bounded(value.get("reason").and_then(Value::as_str).unwrap_or(""), 40),
        "ownersSeen": counter("ownersSeen"),
        "attached": counter("attached"),
        "merged": counter("merged"),
        "elementsDropped": counter("elementsDropped"),
        "skipped": skipped
    })
}

fn rounded(value: Option<&Value>) -> i64 {
    value.and_then(Value::as_f64).unwrap_or(0.0).round() as i64
}

fn decode_screenshot(value: Option<&Value>) -> Result<Option<Screenshot>, ApiError> {
    let Some(data_url) = value.and_then(Value::as_str) else {
        return Ok(None);
    };
    let (content_type, encoded) =
        if let Some(encoded) = data_url.strip_prefix("data:image/png;base64,") {
            ("image/png", encoded)
        } else if let Some(encoded) = data_url.strip_prefix("data:image/jpeg;base64,") {
            ("image/jpeg", encoded)
        } else {
            return Ok(None);
        };
    let bytes = BASE64_STANDARD
        .decode(encoded)
        .map_err(|_| ApiError::bad_request("Screenshot is not valid base64"))?;
    if bytes.len() > MAX_SCREENSHOT_BYTES {
        return Err(ApiError::bad_request("Screenshot exceeds the 8 MB limit"));
    }
    let content_hash = sha256_hex(&bytes);
    let (width, height) = match image_dimensions(content_type, &bytes) {
        Some((width, height)) => (Some(width), Some(height)),
        None => (None, None),
    };
    Ok(Some(Screenshot {
        bytes: Bytes::from(bytes),
        content_type,
        content_hash,
        width,
        height,
        id: Uuid::new_v4().simple().to_string(),
        binding: String::new(),
        route: "",
    }))
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn image_dimensions(content_type: &str, bytes: &[u8]) -> Option<(u32, u32)> {
    match content_type {
        "image/png" => png_dimensions(bytes),
        "image/jpeg" => jpeg_dimensions(bytes),
        _ => None,
    }
}

/// Reads the IHDR width and height of a PNG without decoding pixel data.
fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
    if bytes.len() < 24 || bytes[..8] != PNG_SIGNATURE || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let width = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let height = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    (width > 0 && height > 0).then_some((width, height))
}

/// Scans JPEG markers for the first start-of-frame segment and reads its
/// dimensions without decoding pixel data.
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut index = 2_usize;
    while index + 9 <= bytes.len() {
        if bytes[index] != 0xFF {
            return None;
        }
        let marker = bytes[index + 1];
        match marker {
            0xFF => index += 1,
            0x01 | 0xD0..=0xD8 => index += 2,
            0xD9 => return None,
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                let height = u32::from(u16::from_be_bytes([bytes[index + 5], bytes[index + 6]]));
                let width = u32::from(u16::from_be_bytes([bytes[index + 7], bytes[index + 8]]));
                return (width > 0 && height > 0).then_some((width, height));
            }
            _ => {
                let length = usize::from(u16::from_be_bytes([bytes[index + 2], bytes[index + 3]]));
                if length < 2 {
                    return None;
                }
                index += 2 + length;
            }
        }
    }
    None
}

fn bounded(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

fn now_iso() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

fn bearer_token(headers: &HeaderMap) -> &str {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("")
}

fn session_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Session "))
        .filter(|value| token_is_valid(value))
}

fn is_loopback_host(host: &str, bound_port: u16) -> bool {
    let (hostname, port) = if let Some(bracketed) = host.strip_prefix('[') {
        let Some((hostname, suffix)) = bracketed.split_once(']') else {
            return false;
        };
        match suffix.strip_prefix(':') {
            Some(port) => (hostname, Some(port)),
            None if suffix.is_empty() => (hostname, None),
            None => return false,
        }
    } else {
        match host.split_once(':') {
            Some((hostname, port)) => (hostname, Some(port)),
            None => (host, None),
        }
    };
    if !matches!(hostname, "127.0.0.1" | "localhost" | "::1") {
        return false;
    }
    match port {
        None => true,
        Some(port) => port.parse::<u16>().ok() == Some(bound_port),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_exact_chrome_extension_origins() {
        assert!(valid_extension_origin(
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop"
        ));
        for invalid in [
            "chrome-extension://",
            "chrome-extension://abcdefghijklmnopabcdefghijklmnop/",
            "chrome-extension://abcdefghijklmnopabcdefghijklmnox",
            "chrome-extension://ABCDEFGHIJKLMNOPABCDEFGHIJKLMNOP",
            "https://abcdefghijklmnopabcdefghijklmnop",
        ] {
            assert!(!valid_extension_origin(invalid), "accepted {invalid}");
        }
    }

    #[test]
    fn validates_full_access_action_parameters() {
        let params = sanitize_params(
            "page.clickAt",
            json!({ "tabId": 7, "generation": "g1", "x": 10.5, "y": 20, "button": "right", "clickCount": 2 }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(params["tabId"], 7);
        assert_eq!(params["button"], "right");
        assert!(
            sanitize_params(
                "page.clickAt",
                json!({ "tabId": 7, "generation": "g1", "x": 10, "y": -1 }),
                None,
                None
            )
            .is_err()
        );
        assert!(sanitize_params("page.evaluate", json!({ "tabId": 7 }), None, None).is_err());
    }

    #[test]
    fn accepts_epoch_embedded_and_legacy_element_refs() {
        for valid in ["e1", "e12", "e500", "mfz3k2ab-a1b2c3d4.e12", "g1.e1"] {
            assert!(valid_element_ref(valid), "rejected {valid}");
        }
        for invalid in [
            "", "e", "e0", "e01", "e12345", "x1", "12", ".e1", "g1.", "g1.e", "g1.f1", "G1.e1",
            "g!n.e1", "g1..e1", "g1.e1.e2", "g_1.e1", "e1 ",
        ] {
            assert!(!valid_element_ref(invalid), "accepted {invalid}");
        }

        let embedded = sanitize_params(
            "page.click",
            json!({ "tabId": 7, "ref": "mfz3k2ab-a1b2c3d4.e12", "generation": "mfz3k2ab-a1b2c3d4" }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(embedded["ref"], "mfz3k2ab-a1b2c3d4.e12");
        let legacy = sanitize_params(
            "page.hover",
            json!({ "tabId": 7, "ref": "e3", "generation": "g1" }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(legacy["ref"], "e3");
        let junk = sanitize_params(
            "page.click",
            json!({ "tabId": 7, "ref": "not a ref", "generation": "g1" }),
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(junk.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn frame_scoped_refs_parse_and_bound_the_frame_index() {
        for valid in [
            "e12",
            "g1.e12",
            "g1.f1.e1",
            "g1.f16.e9999",
            "mfz3k2ab-a1b2c3d4.f16.e9999",
        ] {
            assert!(valid_element_ref(valid), "rejected {valid}");
        }
        for invalid in [
            "g1.f0.e1",
            "g1.f17.e1",
            "g1.f01.e1",
            "g1.f99.e1",
            "g1.f2.e0",
            "g1.f2.f3.e1",
            "g1.f2.x1",
            "g1..e1",
            "g1.f2.e1.",
            "G1.f2.e1",
            "g1.f2.e12345",
            "g1.F2.e1",
            "g1.f.e1",
        ] {
            assert!(!valid_element_ref(invalid), "accepted {invalid}");
        }

        // A two-segment ref is always <generation>.<element>. "f2" is a legal
        // generation string, so "f2.e1" stays the legacy top-frame shape and
        // is never reinterpreted as a frame-scoped ref; only the three-segment
        // form addresses a frame.
        assert!(valid_element_ref("f2.e1"));

        let frame_scoped = sanitize_params(
            "page.click",
            json!({ "tabId": 7, "ref": "mfz3k2ab-a1b2c3d4.f2.e5", "generation": "mfz3k2ab-a1b2c3d4" }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(frame_scoped["ref"], "mfz3k2ab-a1b2c3d4.f2.e5");
        let hover = sanitize_params(
            "page.hover",
            json!({ "tabId": 7, "ref": "g1.f16.e9999", "generation": "g1" }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(hover["ref"], "g1.f16.e9999");
        assert!(
            sanitize_params(
                "page.click",
                json!({ "tabId": 7, "ref": "g1.f17.e5", "generation": "g1" }),
                None,
                None
            )
            .is_err()
        );
    }

    #[test]
    fn sanitize_element_carries_bounded_frame_provenance() {
        let framed = sanitize_element(&json!({
            "ref": "g1.f2.e5",
            "role": "button",
            "name": "Pay",
            "frameRef": "f2",
            "frameId": "F".repeat(400),
            "frameUrlOrigin": "https://pay.example.test/".to_owned() + &"x".repeat(600),
            "crossOrigin": true
        }))
        .unwrap();
        assert_eq!(framed.frame_ref.as_deref(), Some("f2"));
        assert_eq!(framed.frame_id.as_deref().unwrap().len(), 100);
        assert_eq!(framed.frame_url_origin.as_deref().unwrap().len(), 300);
        assert_eq!(framed.cross_origin, Some(true));

        // A forged frame key outside f1..f16 loses every provenance field.
        let forged_key = sanitize_element(&json!({
            "ref": "e5", "frameRef": "f99", "frameId": "abc", "crossOrigin": true
        }))
        .unwrap();
        assert_eq!(forged_key.frame_ref, None);
        assert_eq!(forged_key.frame_id, None);
        assert_eq!(forged_key.frame_url_origin, None);
        assert_eq!(forged_key.cross_origin, None);

        // A top-frame element cannot claim cross-origin provenance.
        let forged_flag = sanitize_element(&json!({ "ref": "e5", "crossOrigin": true })).unwrap();
        assert_eq!(forged_flag.cross_origin, None);
        let serialized = serde_json::to_value(&forged_flag).unwrap();
        for absent in ["frameRef", "frameId", "frameUrlOrigin", "crossOrigin"] {
            assert!(serialized.get(absent).is_none(), "leaked {absent}");
        }
    }

    #[test]
    fn sanitize_frame_bounds_the_frame_list_and_skip_report() {
        let frame = sanitize_frame(&json!({
            "ref": "f3",
            "frameId": "6A1B",
            "urlOrigin": "https://pay.example.test",
            "crossOrigin": true,
            "depth": 99,
            "offset": { "x": 120.4, "y": 64.6 },
            "size": { "width": 380, "height": 220 },
            "elementCount": 12,
            "truncated": true
        }))
        .unwrap();
        assert_eq!(frame.reference, "f3");
        assert_eq!(frame.depth, MAX_OBSERVED_FRAME_DEPTH);
        assert_eq!(frame.offset.x, 120);
        assert_eq!(frame.offset.y, 65);
        assert!(frame.truncated);

        assert!(sanitize_frame(&json!({ "ref": "f17", "frameId": "a" })).is_none());
        assert!(sanitize_frame(&json!({ "ref": "e1", "frameId": "a" })).is_none());
        assert!(sanitize_frame(&json!({ "ref": "f1", "frameId": "" })).is_none());

        let frames: Vec<_> = (1..=20)
            .map(|index| json!({ "ref": format!("f{index}"), "frameId": format!("id{index}") }))
            .collect();
        let kept: Vec<_> = frames
            .iter()
            .filter_map(sanitize_frame)
            .take(MAX_OBSERVED_FRAMES)
            .collect();
        assert_eq!(kept.len(), MAX_OBSERVED_FRAMES);

        let skipped: Vec<_> = (0..40)
            .map(|index| {
                json!({ "urlOrigin": format!("https://ad{index}.example.test"), "reason": "x".repeat(80) })
            })
            .collect();
        let summary = sanitize_frame_summary(&json!({
            "supported": true,
            "mode": "cdp-auto-attach",
            "ownersSeen": 99_999_999_u64,
            "attached": 3,
            "merged": 2,
            "elementsDropped": 7,
            "skipped": skipped
        }));
        assert_eq!(summary["supported"], true);
        assert_eq!(summary["ownersSeen"], json!(FRAME_SUMMARY_COUNTER_MAX));
        assert_eq!(
            summary["skipped"].as_array().unwrap().len(),
            MAX_OBSERVED_FRAME_SKIPS
        );
        assert_eq!(summary["skipped"][0]["reason"].as_str().unwrap().len(), 40);
        assert!(sanitize_frame_summary(&json!("not an object")).is_null());
    }

    #[test]
    fn publication_cap_reserves_a_share_for_frame_elements() {
        let top = |count: usize| {
            (1..=count)
                .map(|index| json!({ "ref": format!("g1.e{index}"), "role": "link" }))
                .collect::<Vec<Value>>()
        };
        let framed = |count: usize| {
            (1..=count)
                .map(|index| {
                    json!({ "ref": format!("g1.f1.e{index}"), "role": "link", "frameRef": "f1" })
                })
                .collect::<Vec<Value>>()
        };

        // Under the cap nothing is dropped and nothing is reported.
        let (published, truncated) = publish_elements(&[top(10), framed(5)].concat());
        assert_eq!(published.len(), 15);
        assert!(!truncated);

        // The extension appends frame elements last; a flat cap would publish
        // none of them on a page with 300 top-document elements.
        let (published, truncated) = publish_elements(&[top(300), framed(60)].concat());
        assert!(truncated);
        assert_eq!(published.len(), MAX_PUBLISHED_ELEMENTS);
        let kept_frame_elements = published
            .iter()
            .filter(|element| element.frame_ref.is_some())
            .count();
        assert_eq!(kept_frame_elements, MAX_PUBLISHED_FRAME_ELEMENTS);
        assert_eq!(published[0].reference, "g1.e1");

        // Whichever class runs out first hands its slots to the other, so the
        // cap is always filled.
        let (published, _) = publish_elements(&[top(10), framed(300)].concat());
        assert_eq!(published.len(), MAX_PUBLISHED_ELEMENTS);
        assert_eq!(
            published
                .iter()
                .filter(|element| element.frame_ref.is_some())
                .count(),
            240
        );
        let (published, _) = publish_elements(&[top(300), framed(20)].concat());
        assert_eq!(
            published
                .iter()
                .filter(|element| element.frame_ref.is_some())
                .count(),
            20
        );

        // Truncation reports the publication cap, not the raw array: an
        // observation whose refless entries were dropped by the sanitizer is
        // not truncated.
        let mut refless = top(240);
        refless.extend((0..40).map(|_| json!({ "role": "link" })));
        let (published, truncated) = publish_elements(&refless);
        assert_eq!(published.len(), 240);
        assert!(!truncated);
    }

    #[test]
    fn frame_element_counts_restate_what_was_published() {
        let frames = vec![
            sanitize_frame(&json!({ "ref": "f1", "frameId": "a", "elementCount": 12 })).unwrap(),
            sanitize_frame(&json!({ "ref": "f2", "frameId": "b", "elementCount": 4 })).unwrap(),
        ];
        let published: Vec<ElementInfo> = ["g1.f1.e1", "g1.f1.e2", "g1.e1"]
            .iter()
            .zip(["f1", "f1", ""])
            .filter_map(|(reference, frame_ref)| {
                let mut value = json!({ "ref": reference });
                if !frame_ref.is_empty() {
                    value["frameRef"] = json!(frame_ref);
                }
                sanitize_element(&value)
            })
            .collect();
        let reconciled = reconcile_frame_counts(frames, &published);
        assert_eq!(reconciled[0].element_count, 2);
        assert!(reconciled[0].truncated);
        // A frame whose elements all fell outside the cap stays visible, and
        // says so, instead of advertising refs that are not there.
        assert_eq!(reconciled[1].element_count, 0);
        assert!(reconciled[1].truncated);
    }

    #[test]
    fn frameless_observations_serialize_exactly_as_before_frame_support() {
        let observation = Observation {
            tab_id: 7,
            captured_at: "now".to_owned(),
            title: "Test tab".to_owned(),
            url: "https://example.test/".to_owned(),
            generation: "g1".to_owned(),
            viewport: Value::Null,
            scroll: Value::Null,
            selected_text: String::new(),
            body_text: String::new(),
            elements: Vec::new(),
            frames: None,
            frame_summary: None,
            elements_truncated: None,
        };
        let serialized = serde_json::to_value(&observation).unwrap();
        for absent in ["frames", "frameSummary", "elementsTruncated"] {
            assert!(serialized.get(absent).is_none(), "leaked {absent}");
        }

        let framed = Observation {
            frames: Some(vec![
                sanitize_frame(&json!({ "ref": "f1", "frameId": "abc" })).unwrap(),
            ]),
            frame_summary: Some(sanitize_frame_summary(&json!({ "supported": true }))),
            elements_truncated: Some(true),
            ..observation
        };
        let serialized = serde_json::to_value(&framed).unwrap();
        assert_eq!(serialized["frames"][0]["ref"], "f1");
        assert_eq!(serialized["frames"][0]["crossOrigin"], true);
        assert_eq!(serialized["frameSummary"]["mode"], "cdp-auto-attach");
        assert_eq!(serialized["elementsTruncated"], true);
    }

    #[test]
    fn validates_click_pointer_options() {
        let defaults = sanitize_params(
            "page.click",
            json!({ "tabId": 7, "ref": "e1", "generation": "g1" }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(defaults["button"], "left");
        assert_eq!(defaults["clickCount"], 1);
        assert_eq!(defaults["modifiers"], json!([]));

        let modified = sanitize_params(
            "page.click",
            json!({
                "tabId": 7,
                "ref": "g1.e1",
                "generation": "g1",
                "button": "middle",
                "clickCount": 3,
                "modifiers": ["Shift", "Meta", "Shift"]
            }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(modified["button"], "middle");
        assert_eq!(modified["clickCount"], 3);
        assert_eq!(modified["modifiers"], json!(["Shift", "Meta"]));

        for params in [
            json!({ "tabId": 7, "ref": "e1", "generation": "g1", "button": "back" }),
            json!({ "tabId": 7, "ref": "e1", "generation": "g1", "clickCount": 0 }),
            json!({ "tabId": 7, "ref": "e1", "generation": "g1", "clickCount": 4 }),
            json!({ "tabId": 7, "ref": "e1", "generation": "g1", "modifiers": ["Hyper"] }),
            json!({ "tabId": 7, "ref": "e1", "generation": "g1", "modifiers": "Shift" }),
            json!({ "tabId": 7, "ref": "e1", "generation": "g1", "modifiers": [8] }),
        ] {
            assert!(
                sanitize_params("page.click", params.clone(), None, None).is_err(),
                "accepted {params}"
            );
        }
    }

    #[test]
    fn normalizes_vendor_and_canonical_key_chords() {
        for (input, expected) in [
            ("Meta+L", "Meta+L"),
            ("cmd+l", "Meta+L"),
            ("ctrl+shift+t", "Control+Shift+T"),
            ("shift+ctrl+t", "Control+Shift+T"),
            ("Control+Shift+T", "Control+Shift+T"),
            ("esc", "Escape"),
            ("Escape", "Escape"),
            ("return", "Enter"),
            ("del", "Delete"),
            ("space", "Space"),
            ("alt+f4", "Alt+F4"),
            ("option+Tab", "Alt+Tab"),
            ("win+d", "Meta+D"),
            ("super+ArrowLeft", "Meta+ArrowLeft"),
            ("ctrl+ctrl+a", "Control+A"),
            ("Control+.", "Control+."),
            ("9", "9"),
            // A bare single letter keeps its case verbatim (uppercasing
            // would imply Shift for Gmail-style j/k shortcuts); letters
            // inside modifier chords keep the canonical uppercase form.
            ("j", "j"),
            ("J", "J"),
            ("shift+j", "Shift+J"),
            ("ctrl+j", "Control+J"),
            ("Shift+j", "Shift+J"),
        ] {
            assert_eq!(
                normalize_key_chord(input).unwrap(),
                expected,
                "chord {input}"
            );
        }
        for invalid in [
            "",
            "+",
            "ctrl+",
            "hyper+l",
            "ctrl+banana",
            "banana",
            "ctrl+alt+shift+meta+a",
            "F13",
            "f0",
        ] {
            assert!(normalize_key_chord(invalid).is_err(), "accepted {invalid}");
        }

        let page_key = sanitize_params(
            "page.key",
            json!({ "tabId": 7, "generation": "g1", "key": "cmd+l" }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(page_key["key"], "Meta+L");
        let computer_key = sanitize_computer_params(
            "computer.key",
            json!({ "frameId": "frame-1", "key": "ctrl+shift+t" }),
            None,
        )
        .unwrap();
        assert_eq!(computer_key["key"], "Control+Shift+T");
        assert!(
            sanitize_computer_params(
                "computer.key",
                json!({ "frameId": "frame-1", "key": "hyper+l" }),
                None,
            )
            .is_err()
        );
    }

    #[test]
    fn clamps_wait_for_timeouts_and_requires_a_condition() {
        let clamped = sanitize_params(
            "page.waitFor",
            json!({ "tabId": 7, "text": "Welcome", "timeoutMs": 99_999 }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(clamped["timeoutMs"], 12_000);
        assert_eq!(clamped["text"], "Welcome");
        assert!(clamped.get("controlSessionId").is_none());

        let floored = sanitize_params(
            "page.waitFor",
            json!({ "tabId": 7, "urlPrefix": "https://example.test/", "timeoutMs": 1 }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(floored["timeoutMs"], 100);

        let defaulted = sanitize_params(
            "page.waitFor",
            json!({ "tabId": 7, "textGone": "Loading", "mutationQuietMs": 300 }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(defaulted["timeoutMs"], 5_000);
        assert_eq!(defaulted["mutationQuietMs"], 300);

        for params in [
            json!({ "tabId": 7 }),
            json!({ "tabId": 7, "text": "" }),
            json!({ "tabId": 7, "mutationQuietMs": 100 }),
            json!({ "tabId": 7, "mutationQuietMs": 5_001 }),
        ] {
            assert!(
                sanitize_params("page.waitFor", params.clone(), None, None).is_err(),
                "accepted {params}"
            );
        }
    }

    #[test]
    fn batch_sanitizer_validates_sub_actions_and_binds_leases_once() {
        let mut params = sanitize_params(
            "page.batch",
            json!({
                "tabId": 7,
                "generation": "g1",
                "controlSessionId": "control-1",
                "actions": [
                    { "method": "page.fill", "ref": "g1.e1", "text": "hello", "turn": 99, "moveSequence": 3 },
                    { "method": "page.key", "key": "ctrl+a", "generation": "g1" },
                    { "method": "page.scroll", "deltaY": 120.7 },
                    { "method": "page.click", "ref": "e2", "button": "middle", "tabId": 7 }
                ]
            }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(params["tabId"], 7);
        assert_eq!(params["generation"], "g1");
        assert_eq!(params["controlSessionId"], "control-1");
        let actions = params["actions"].as_array().unwrap().clone();
        assert_eq!(actions.len(), 4);
        for (index, action) in actions.iter().enumerate() {
            assert_eq!(action["tabId"], 7, "actions[{index}] lost the batch tab");
            assert_eq!(action["generation"], "g1", "actions[{index}] generation");
            // Per-step params are sanitized, but lease bindings never are:
            // they are injected exactly once at the batch top level.
            for binding in ["controlSessionId", "turn", "moveSequence"] {
                assert!(
                    action.get(binding).is_none(),
                    "actions[{index}] carries a per-step {binding}"
                );
            }
        }
        assert_eq!(actions[0]["method"], "page.fill");
        assert_eq!(actions[0]["text"], "hello");
        assert_eq!(actions[1]["key"], "Control+A");
        assert_eq!(actions[2]["deltaY"], 120.0);
        assert_eq!(actions[3]["button"], "middle");
        assert_eq!(actions[3]["clickCount"], 1);
        assert_eq!(actions[3]["modifiers"], json!([]));

        // bind_browser_control fills only the missing top-level bindings.
        bind_browser_control(
            &mut params,
            &json!({ "active": true, "sessionId": "control-2", "turn": 4, "moveSequence": 9 }),
            true,
        );
        assert_eq!(params["controlSessionId"], "control-1");
        assert_eq!(params["turn"], 4);
        assert_eq!(params["moveSequence"], 9);
        for action in params["actions"].as_array().unwrap() {
            assert!(action.get("controlSessionId").is_none());
            assert!(action.get("turn").is_none());
        }

        let eleven: Vec<Value> = (0..11)
            .map(|_| json!({ "method": "page.key", "key": "Enter" }))
            .collect();
        for params in [
            json!({ "tabId": 7, "generation": "g1", "actions": [] }),
            json!({ "tabId": 7, "generation": "g1", "actions": eleven }),
            json!({ "tabId": 7, "generation": "g1" }),
            json!({ "tabId": 7, "actions": [{ "method": "page.key", "key": "Enter" }] }),
            json!({ "tabId": 7, "generation": "g1", "actions": [{ "method": "page.batch", "actions": [] }] }),
            json!({ "tabId": 7, "generation": "g1", "actions": [{ "method": "page.evaluate", "expression": "1" }] }),
            json!({ "tabId": 7, "generation": "g1", "actions": [{ "method": "page.navigate", "url": "https://example.test/" }] }),
            json!({ "tabId": 7, "generation": "g1", "actions": [{ "method": "page.observe" }] }),
            json!({ "tabId": 7, "generation": "g1", "actions": [{ "method": "page.handleDialog", "accept": true }] }),
            json!({ "tabId": 7, "generation": "g1", "actions": [{ "method": "page.click", "ref": "not a ref" }] }),
            json!({ "tabId": 7, "generation": "g1", "actions": [{ "method": "page.fill", "ref": "e1", "text": "x", "generation": "g0" }] }),
            json!({ "tabId": 7, "generation": "g1", "actions": [{ "method": "page.fill", "ref": "e1", "text": "x", "tabId": 8 }] }),
            json!({ "tabId": 7, "generation": "g1", "actions": ["page.click"] }),
        ] {
            let error = sanitize_params("page.batch", params.clone(), None, None)
                .expect_err(&format!("accepted {params}"));
            assert_eq!(error.status, StatusCode::BAD_REQUEST);
        }
    }

    #[test]
    fn handle_dialog_sanitizer_and_gate_cover_the_published_surface() {
        let accepted = sanitize_params(
            "page.handleDialog",
            json!({ "tabId": 7, "accept": true, "promptText": "reply" }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(accepted["accept"], true);
        assert_eq!(accepted["promptText"], "reply");
        let dismissed = sanitize_params(
            "page.handleDialog",
            json!({ "tabId": 7, "accept": false }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(dismissed["accept"], false);
        assert!(dismissed.get("promptText").is_none());
        for params in [
            json!({ "tabId": 7 }),
            json!({ "tabId": 7, "accept": "yes" }),
            json!({ "tabId": 7, "accept": 1 }),
            json!({ "tabId": 7, "accept": true, "promptText": "x".repeat(1_001) }),
        ] {
            assert!(
                sanitize_params("page.handleDialog", params.clone(), None, None).is_err(),
                "accepted {params}"
            );
        }

        let dialog = sanitize_pending_dialog(&json!({
            "type": "confirm",
            "message": "Proceed?",
            "hasPrompt": false,
            "at": 123,
            "tabId": 7
        }));
        assert_eq!(dialog["type"], "confirm");
        assert_eq!(dialog["message"], "Proceed?");
        assert_eq!(dialog["hasPrompt"], false);
        assert_eq!(dialog["at"], 123);
        assert_eq!(dialog["tabId"], 7);
        let junk = sanitize_pending_dialog(&json!({
            "type": "weird",
            "message": "m".repeat(600)
        }));
        assert_eq!(junk["type"], "dialog");
        assert_eq!(junk["message"].as_str().unwrap().chars().count(), 500);
        let prompt = sanitize_pending_dialog(&json!({ "type": "prompt", "message": "Name?" }));
        assert_eq!(prompt["hasPrompt"], true);

        for allowed in [
            "status",
            "tabs.list",
            "page.handleDialog",
            "browser.control.status",
            "browser.control.stop",
        ] {
            assert!(dialog_tolerant_method(allowed), "{allowed} must stay open");
        }
        // page.observe and page.waitFor would hang against the dialog-frozen
        // renderer and revoke the lease by timeout, and browser.control.start
        // initializes the lease document through the renderer, so all three
        // are gated alongside every mutation.
        for blocked in [
            "page.observe",
            "page.waitFor",
            "browser.control.start",
            "page.click",
            "page.batch",
            "page.navigate",
            "page.fill",
            "page.select",
            "page.key",
            "page.scroll",
            "page.clickAt",
            "page.typeText",
            "page.evaluate",
            "page.hover",
            "page.back",
            "page.forward",
            "page.reload",
            "tabs.activate",
            "tabs.new",
            "tabs.close",
        ] {
            assert!(!dialog_tolerant_method(blocked), "{blocked} must be gated");
        }
    }

    #[test]
    fn converts_normalized1000_click_coordinates_against_the_observed_viewport() {
        let params = sanitize_params(
            "page.clickAt",
            json!({
                "tabId": 7,
                "generation": "g1",
                "x": 500,
                "y": 250,
                "coordinateSpace": "normalized1000"
            }),
            None,
            Some((800.0, 600.0)),
        )
        .unwrap();
        assert_eq!(params["x"], 400.0);
        assert_eq!(params["y"], 150.0);
        assert!(params.get("coordinateSpace").is_none());

        // The default space keeps raw viewport coordinates untouched.
        let untouched = sanitize_params(
            "page.clickAt",
            json!({ "tabId": 7, "generation": "g1", "x": 500, "y": 250 }),
            None,
            Some((800.0, 600.0)),
        )
        .unwrap();
        assert_eq!(untouched["x"], 500.0);

        // Without an observation, normalized coordinates cannot be grounded.
        let missing = sanitize_params(
            "page.clickAt",
            json!({ "tabId": 7, "generation": "g1", "x": 1, "y": 1, "coordinateSpace": "normalized1000" }),
            None,
            None,
        )
        .unwrap_err();
        assert_eq!(missing.code, "NO_BROWSER_OBSERVATION");
        assert_eq!(missing.status, StatusCode::CONFLICT);

        // A degenerate viewport is rejected with the same clear error.
        let degenerate = sanitize_params(
            "page.clickAt",
            json!({ "tabId": 7, "generation": "g1", "x": 1, "y": 1, "coordinateSpace": "normalized1000" }),
            None,
            Some((0.0, 600.0)),
        )
        .unwrap_err();
        assert_eq!(degenerate.code, "NO_BROWSER_OBSERVATION");

        // Values beyond the 0..=1000 envelope and unknown spaces are refused.
        assert!(
            sanitize_params(
                "page.clickAt",
                json!({ "tabId": 7, "generation": "g1", "x": 1001, "y": 1, "coordinateSpace": "normalized1000" }),
                None,
                Some((800.0, 600.0)),
            )
            .is_err()
        );
        assert!(
            sanitize_params(
                "page.clickAt",
                json!({ "tabId": 7, "generation": "g1", "x": 1, "y": 1, "coordinateSpace": "screen" }),
                None,
                Some((800.0, 600.0)),
            )
            .is_err()
        );
    }

    #[test]
    fn normalized1000_conversion_round_trips_and_clamps() {
        for (value, extent, expected) in [
            (0.0, 640.0, 0.0),
            // The boundary value 1000 converts to the last addressable
            // pixel, because downstream validators reject x >= extent.
            (1_000.0, 640.0, 639.0),
            (500.0, 640.0, 320.0),
            (250.0, 400.0, 100.0),
            (1_000.0, 1.0, 0.0),
        ] {
            assert_eq!(normalized1000_to_pixels(value, extent), expected);
        }
        // Round trip: pixels -> normalized -> pixels stays within float noise
        // for every addressable pixel.
        for pixels in [0.0, 1.0, 123.456, 639.0] {
            let normalized = pixels / 640.0 * NORMALIZED_COORDINATE_MAX;
            assert!((normalized1000_to_pixels(normalized, 640.0) - pixels).abs() < 1e-9);
        }
        // Out-of-range inputs clamp to the addressable image bounds.
        assert_eq!(normalized1000_to_pixels(1_000.000_1, 640.0), 639.0);
        assert_eq!(normalized1000_to_pixels(-0.000_1, 640.0), 0.0);
    }

    #[test]
    fn converts_normalized1000_computer_coordinates_against_the_stored_frame() {
        let params = sanitize_computer_params(
            "computer.click",
            json!({
                "frameId": "frame-1",
                "x": 500,
                "y": 250,
                "coordinateSpace": "normalized1000"
            }),
            Some((640, 400)),
        )
        .unwrap();
        assert_eq!(params["x"], 320.0);
        assert_eq!(params["y"], 100.0);
        assert!(params.get("coordinateSpace").is_none());

        let drag = sanitize_computer_params(
            "computer.drag",
            json!({
                "frameId": "frame-1",
                "fromX": 0,
                "fromY": 0,
                "toX": 1000,
                "toY": 1000,
                "coordinateSpace": "normalized1000"
            }),
            Some((640, 400)),
        )
        .unwrap();
        // The 1000 boundary lands on the last addressable pixel, which every
        // downstream `< extent` validator still accepts.
        assert_eq!(drag["toX"], 639.0);
        assert_eq!(drag["toY"], 399.0);

        // The default image space is untouched by the stored frame size.
        let untouched = sanitize_computer_params(
            "computer.move",
            json!({ "frameId": "frame-1", "x": 500, "y": 250 }),
            Some((640, 400)),
        )
        .unwrap();
        assert_eq!(untouched["x"], 500.0);

        let missing = sanitize_computer_params(
            "computer.move",
            json!({ "frameId": "frame-1", "x": 1, "y": 1, "coordinateSpace": "normalized1000" }),
            None,
        )
        .unwrap_err();
        assert_eq!(missing.code, "NO_COMPUTER_FRAME");
        assert_eq!(missing.status, StatusCode::CONFLICT);

        assert!(
            sanitize_computer_params(
                "computer.move",
                json!({ "frameId": "frame-1", "x": 1000.5, "y": 1, "coordinateSpace": "normalized1000" }),
                Some((640, 400)),
            )
            .is_err()
        );
        assert!(
            sanitize_computer_params(
                "computer.move",
                json!({ "frameId": "frame-1", "x": 1, "y": 1, "coordinateSpace": "screen" }),
                Some((640, 400)),
            )
            .is_err()
        );
    }

    #[test]
    fn preserves_and_binds_browser_control_revisions() {
        let mut params = sanitize_params(
            "page.scroll",
            json!({
                "tabId": 7,
                "generation": "g2",
                "deltaY": 120,
                "controlSessionId": "control-1",
                "turn": 4,
                "moveSequence": 9
            }),
            None,
            None,
        )
        .unwrap();
        assert_eq!(params["generation"], "g2");
        assert_eq!(params["controlSessionId"], "control-1");
        assert_eq!(params["turn"], 4);
        assert_eq!(params["moveSequence"], 9);

        params.as_object_mut().unwrap().remove("moveSequence");
        bind_browser_control(
            &mut params,
            &json!({
                "active": true,
                "sessionId": "control-1",
                "turn": 4,
                "moveSequence": 10
            }),
            true,
        );
        assert_eq!(params["moveSequence"], 10);
        assert_eq!(params["turn"], 4);
    }

    #[test]
    fn accepts_only_monotonic_events_for_the_exact_transport_session() {
        let connection_id = Uuid::new_v4();
        let mut last = 0;
        let first = json!({
            "protocolVersion": PROTOCOL_VERSION,
            "sessionId": connection_id.to_string(),
            "eventSequence": 2
        });
        assert!(session_event_valid(&first, connection_id, &mut last));
        assert_eq!(last, 2);
        assert!(!session_event_valid(&first, connection_id, &mut last));
        assert!(!session_event_valid(
            &json!({
                "protocolVersion": PROTOCOL_VERSION,
                "sessionId": Uuid::new_v4().to_string(),
                "eventSequence": 3
            }),
            connection_id,
            &mut last,
        ));
        assert_eq!(last, 2);
    }

    #[test]
    fn enforces_loopback_host_boundary() {
        assert!(is_loopback_host("127.0.0.1:17373", 17_373));
        assert!(is_loopback_host("localhost:17373", 17_373));
        assert!(is_loopback_host("[::1]:17373", 17_373));
        assert!(is_loopback_host("127.0.0.1", 17_373));
        assert!(is_loopback_host("localhost", 17_373));
        assert!(is_loopback_host("[::1]", 17_373));
        for rejected in [
            "example.com:17373",
            "evil.example",
            "127.0.0.1:9999",
            "localhost:9999",
            "[::1]:9999",
            "127.0.0.1:",
            "[::1]x",
            "[::1]:17373x",
            "127.0.0.1.evil.example:17373",
            "LOCALHOST:17373",
            "::1",
            "",
        ] {
            assert!(!is_loopback_host(rejected, 17_373), "accepted {rejected}");
        }
    }

    #[test]
    fn expired_dashboard_session_cannot_mutate_even_with_its_old_csrf_token() {
        let state = AppState::new(create_token(), 17_373, Duration::from_secs(1), false);
        let session_id = create_token();
        state.sessions.lock().unwrap().insert(
            session_id.clone(),
            Session {
                csrf: create_token(),
                touched_at: OffsetDateTime::now_utc().unix_timestamp() - 12 * 60 * 60 - 1,
            },
        );
        let csrf = state.sessions.lock().unwrap()[&session_id].csrf.clone();
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            format!("Session {session_id}").parse().unwrap(),
        );
        headers.insert("x-csrf-token", csrf.parse().unwrap());
        headers.insert(HOST, "127.0.0.1:17373".parse().unwrap());
        headers.insert(ORIGIN, "http://127.0.0.1:17373".parse().unwrap());
        let error = state.assert_ui_mutation(&headers).unwrap_err();
        assert_eq!(error.status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn decodes_bounded_screenshots() {
        let screenshot = decode_screenshot(Some(&Value::String(
            "data:image/png;base64,iVBORw0KGgo=".to_owned(),
        )))
        .unwrap()
        .unwrap();
        assert_eq!(screenshot.content_type, "image/png");
        assert!(!screenshot.bytes.is_empty());
        assert_eq!(screenshot.content_hash.len(), 64);
        assert!(
            screenshot
                .content_hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit())
        );
        // A truncated PNG has no readable IHDR dimensions.
        assert_eq!(screenshot.width, None);
        assert_eq!(screenshot.height, None);
    }

    #[test]
    fn extracts_stable_content_hashes_and_dimensions_from_screenshots() {
        const PIXEL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";
        let first = decode_screenshot(Some(&Value::String(PIXEL.to_owned())))
            .unwrap()
            .unwrap();
        let second = decode_screenshot(Some(&Value::String(PIXEL.to_owned())))
            .unwrap()
            .unwrap();
        assert_eq!(first.content_hash, second.content_hash);
        assert_eq!(first.content_hash, sha256_hex(&first.bytes));
        assert_eq!(first.width, Some(1));
        assert_eq!(first.height, Some(1));
        assert_eq!(png_dimensions(&first.bytes), Some((1, 1)));
        assert_eq!(jpeg_dimensions(&first.bytes), None);
        // A minimal JPEG header: SOI, then an SOF0 segment with 2x3 pixels.
        let jpeg = [
            0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x0B, 0x08, 0x00, 0x03, 0x00, 0x02, 0x01, 0x01, 0x11,
            0x00,
        ];
        assert_eq!(jpeg_dimensions(&jpeg), Some((2, 3)));
        assert_eq!(png_dimensions(&jpeg), None);
    }

    #[test]
    fn replay_cache_deduplicates_in_flight_and_completed_calls() {
        let now = Instant::now();
        let mut replay = CommandReplay::new();
        assert!(matches!(
            replay.admit("call-1", "fp-1", now),
            ReplayAdmission::New
        ));
        assert!(matches!(
            replay.admit("call-1", "fp-1", now),
            ReplayAdmission::InFlight
        ));
        assert!(matches!(
            replay.admit("call-2", "fp-2", now),
            ReplayAdmission::New
        ));
        replay.complete("call-1", StatusCode::OK, json!({ "ok": true }), now);
        match replay.admit("call-1", "fp-1", now) {
            ReplayAdmission::Replay { status, body } => {
                assert_eq!(status, StatusCode::OK);
                assert_eq!(body, json!({ "ok": true }));
            }
            _ => panic!("completed call must replay"),
        }
    }

    #[test]
    fn replay_cache_refuses_a_call_id_reused_for_a_different_command() {
        let now = Instant::now();
        let mut replay = CommandReplay::new();
        // An in-flight callId with a different fingerprint is reuse, not a
        // duplicate of the pending command.
        assert!(matches!(
            replay.admit("call-1", "fp-a", now),
            ReplayAdmission::New
        ));
        assert!(matches!(
            replay.admit("call-1", "fp-b", now),
            ReplayAdmission::Reused
        ));
        assert!(matches!(
            replay.admit("call-1", "fp-a", now),
            ReplayAdmission::InFlight
        ));
        // A completed callId replays only the exact same command; any other
        // fingerprint is refused instead of returning the cached outcome.
        replay.complete("call-1", StatusCode::OK, json!({ "ok": true }), now);
        assert!(matches!(
            replay.admit("call-1", "fp-b", now),
            ReplayAdmission::Reused
        ));
        assert!(matches!(
            replay.admit("call-1", "fp-a", now),
            ReplayAdmission::Replay { .. }
        ));
        assert_ne!(
            command_fingerprint("page.click", &json!({ "ref": "e1" })),
            command_fingerprint("page.hover", &json!({ "ref": "e1" })),
        );
        assert_ne!(
            command_fingerprint("page.click", &json!({ "ref": "e1" })),
            command_fingerprint("page.click", &json!({ "ref": "e2" })),
        );
        assert_eq!(
            command_fingerprint("page.click", &json!({ "a": 1, "b": 2 })),
            command_fingerprint("page.click", &json!({ "b": 2, "a": 1 })),
        );
    }

    #[test]
    fn dropped_in_flight_guard_caches_an_outcome_unknown_failure() {
        let now = Instant::now();
        let replay = Arc::new(Mutex::new(CommandReplay::new()));
        assert!(matches!(
            replay.lock().unwrap().admit("call-1", "fp-1", now),
            ReplayAdmission::New
        ));
        drop(InFlightCallGuard {
            replay: replay.clone(),
            call_id: Some("call-1".to_owned()),
        });
        // A retry of the interrupted command replays the synthetic
        // outcome-unknown failure and is never admitted as New again.
        match replay.lock().unwrap().admit("call-1", "fp-1", now) {
            ReplayAdmission::Replay { status, body } => {
                assert_eq!(status, StatusCode::GATEWAY_TIMEOUT);
                assert_eq!(body["ok"], false);
                assert_eq!(body["error"]["code"], "COMMAND_OUTCOME_UNKNOWN");
                assert_eq!(body["taxonomy"]["code"], "outcome_unknown");
                assert_eq!(body["taxonomy"]["retriable"], false);
                assert_eq!(body["taxonomy"]["recoveryHint"], "reobserve");
                assert_eq!(body["callId"], "call-1");
            }
            _ => panic!("a dropped guard must cache an outcome-unknown failure"),
        }
        // Reusing the interrupted callId for a different command stays refused.
        assert!(matches!(
            replay.lock().unwrap().admit("call-1", "fp-2", now),
            ReplayAdmission::Reused
        ));
        // A disarmed guard leaves the real completion in charge.
        let mut guard = InFlightCallGuard {
            replay: replay.clone(),
            call_id: Some("call-2".to_owned()),
        };
        assert!(matches!(
            replay.lock().unwrap().admit("call-2", "fp-2", now),
            ReplayAdmission::New
        ));
        assert_eq!(guard.disarm().as_deref(), Some("call-2"));
        drop(guard);
        replay
            .lock()
            .unwrap()
            .complete("call-2", StatusCode::OK, json!({ "ok": true }), now);
        assert!(matches!(
            replay.lock().unwrap().admit("call-2", "fp-2", now),
            ReplayAdmission::Replay { .. }
        ));
    }

    #[test]
    fn replay_cache_expires_entries_after_the_ttl() {
        let now = Instant::now();
        let mut replay = CommandReplay::new();
        assert!(matches!(
            replay.admit("call-1", "fp-1", now),
            ReplayAdmission::New
        ));
        replay.complete("call-1", StatusCode::OK, json!({ "ok": true }), now);
        let before_expiry = now + REPLAY_CACHE_TTL - Duration::from_secs(1);
        assert!(matches!(
            replay.admit("call-1", "fp-1", before_expiry),
            ReplayAdmission::Replay { .. }
        ));
        let after_expiry = now + REPLAY_CACHE_TTL;
        assert!(matches!(
            replay.admit("call-1", "fp-1", after_expiry),
            ReplayAdmission::New
        ));
    }

    #[test]
    fn replay_cache_evicts_the_least_recently_used_entry_at_capacity() {
        let now = Instant::now();
        let mut replay = CommandReplay::new();
        for index in 0..REPLAY_CACHE_ENTRIES {
            let call_id = format!("call-{index}");
            assert!(matches!(
                replay.admit(&call_id, "fp", now),
                ReplayAdmission::New
            ));
            replay.complete(&call_id, StatusCode::OK, json!({ "index": index }), now);
        }
        // Touch the oldest entry so the second-oldest becomes least recent.
        assert!(matches!(
            replay.admit("call-0", "fp", now),
            ReplayAdmission::Replay { .. }
        ));
        assert!(matches!(
            replay.admit("call-overflow", "fp", now),
            ReplayAdmission::New
        ));
        replay.complete("call-overflow", StatusCode::OK, json!({ "ok": true }), now);
        assert!(matches!(
            replay.admit("call-0", "fp", now),
            ReplayAdmission::Replay { .. }
        ));
        assert!(matches!(
            replay.admit("call-1", "fp", now),
            ReplayAdmission::New
        ));
        assert!(matches!(
            replay.admit("call-2", "fp", now),
            ReplayAdmission::Replay { .. }
        ));
    }

    #[test]
    fn browser_control_sanitizer_preserves_human_pause_authority() {
        let control = sanitize_browser_control(&json!({
            "active": false,
            "humanPaused": true,
            "humanPause": {
                "paused": true,
                "reason": "canceled_by_user",
                "at": 1_787_126_488_003_u64,
                "tabId": 17_u64,
                "sessionId": "must-not-be-published"
            },
            "revocationPending": true,
            "revocation": {
                "reason": "canceled_by_user",
                "requiresExplicitStart": true
            }
        }));
        assert_eq!(control["active"], false);
        assert_eq!(control["humanPaused"], true);
        assert_eq!(control["humanPause"]["reason"], "canceled_by_user");
        assert_eq!(control["humanPause"]["tabId"], 17);
        assert!(control["humanPause"].get("sessionId").is_none());
        assert_eq!(control["revocationPending"], true);
    }

    #[test]
    fn preserves_exact_window_identity_through_computer_sanitizers() {
        let params = sanitize_computer_params(
            "computer.observe",
            json!({ "windowId": "47782", "displayId": "legacy" }),
            None,
        )
        .unwrap();
        assert_eq!(params, json!({ "windowId": "47782" }));

        let pointer = json!({
            "id": "cursor-1",
            "visible": true,
            "windowId": "47782",
            "imageX": 604.5,
            "imageY": 413.0,
            "screenX": 540,
            "screenY": 1014,
            "headingDegrees": 45.0,
            "action": "click",
            "pressed": false,
            "sequence": 3,
            "updatedAt": "2026-08-18T00:00:01Z",
            "coordinateSpace": "image-pixels",
            "style": { "theme": "lbb.session-pointer.v1", "fill": "#26C6FF", "outline": "#FFFFFF", "logicalSize": 42, "hotspot": "tip" }
        });
        let frame = json!({
            "id": "frame-1",
            "capturedAt": "2026-08-18T00:00:00Z",
            "windowId": "47782",
            "pid": 51641,
            "appName": "Fixture",
            "windowTitle": "Background target",
            "sessionMode": "background-window",
            "deliveryMode": "exact-window-background",
            "displayId": "47782",
            "displayIndex": 0,
            "displayName": "Fixture — Background target",
            "imageWidth": 1209,
            "imageHeight": 826,
            "screenX": 180,
            "screenY": 768,
            "screenWidth": 720,
            "screenHeight": 492,
            "scaleFactor": 1.0,
            "rotation": 0.0,
            "semanticMode": "macos-accessibility",
            "semanticAvailable": true,
            "pointer": pointer,
            "elements": [{
                "ref": "a1",
                "role": "AXButton",
                "name": "Semantic action",
                "value": null,
                "enabled": true,
                "actions": ["press"],
                "bounds": { "x": 200.0, "y": 800.0, "width": 120.0, "height": 32.0 }
            }]
        });
        let observation = sanitize_computer_observation(Some(&frame)).unwrap();
        assert_eq!(observation.window_id, "47782");
        assert_eq!(observation.pid, 51641);
        assert_eq!(
            observation.share,
            json!({ "active": false }),
            "a frame without a share block reports an inactive share"
        );
        assert_eq!(observation.session_mode, "background-window");
        assert_eq!(observation.delivery_mode, "exact-window-background");
        assert_eq!(observation.semantic_mode, "macos-accessibility");
        assert!(observation.semantic_available);
        assert_eq!(observation.elements.len(), 1);
        assert_eq!(observation.elements[0].reference, "a1");
        assert!(observation.pointer.visible);
        assert_eq!(observation.pointer.sequence, 3);

        let invoke = sanitize_computer_params(
            "computer.invoke",
            json!({ "frameId": "frame-1", "elementRef": "a1", "action": "press" }),
            None,
        )
        .unwrap();
        assert_eq!(invoke["elementRef"], "a1");
        let set_value = sanitize_computer_params(
            "computer.setValue",
            json!({ "frameId": "frame-1", "elementRef": "a2", "value": "hello" }),
            None,
        )
        .unwrap();
        assert_eq!(set_value["value"], "hello");

        let windows = sanitize_computer_windows(Some(&json!([{
            "id": "47782",
            "pid": 51641,
            "appName": "Fixture",
            "title": "Background target",
            "width": 720,
            "height": 492,
            "focused": false
        }])));
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].id, "47782");
        assert!(!windows[0].focused);
    }

    #[test]
    fn share_status_sanitizer_reports_ack_pacing_metrics() {
        let sanitized = sanitize_share_status(Some(&json!({
            "active": true,
            "id": "share-1",
            "windowId": "window-9",
            "fps": 4,
            "sequence": 12,
            "startedAt": "2026-08-19T00:00:00Z",
            "droppedFrames": 3,
            "ackPaced": true,
            "lastAckedSequence": 11,
            "backpressure": "latest-frame-wins"
        })));
        assert_eq!(sanitized["droppedFrames"], 3);
        assert_eq!(sanitized["ackPaced"], true);
        assert_eq!(sanitized["lastAckedSequence"], 11);
        assert_eq!(sanitized["backpressure"], "latest-frame-wins");

        let legacy = sanitize_share_status(Some(&json!({
            "active": true,
            "id": "share-2",
            "windowId": "window-9",
            "fps": 4,
            "sequence": 2,
            "startedAt": "2026-08-19T00:00:00Z"
        })));
        assert_eq!(legacy["ackPaced"], false, "legacy helpers stay unpaced");
        assert_eq!(legacy["lastAckedSequence"], 0);
        assert_eq!(legacy["droppedFrames"], 0);
        assert_eq!(legacy["backpressure"], "producer-blocking");

        assert_eq!(
            share_frame_ack_sequence(&json!({ "frame": { "share": { "sequence": 9 } } })),
            Some(9)
        );
        assert_eq!(
            share_frame_ack_sequence(&json!({ "frame": { "share": { "sequence": "9" } } })),
            None
        );
        assert_eq!(share_frame_ack_sequence(&json!({ "frame": {} })), None);
    }

    #[test]
    fn computer_element_sanitizer_never_forwards_a_sensitive_value() {
        let payload = json!([{
            "ref": "secret-1",
            "role": "AXTextField",
            "name": "Password",
            "value": "must-not-cross-the-server",
            "sensitive": true,
            "valueRedacted": false,
            "actions": ["setValue", "press"],
            "coordinateSpace": "screen-points"
        }]);
        let elements = sanitize_computer_elements(Some(&payload));
        assert_eq!(elements.len(), 1);
        let sensitive = &elements[0];
        assert!(sensitive.sensitive);
        assert!(sensitive.value_redacted);
        assert!(sensitive.value.is_none());
        assert!(!sensitive.actions.iter().any(|action| action == "setValue"));
        assert!(sensitive.actions.iter().any(|action| action == "press"));
    }

    /// The exact path a connector failure takes: the hub hands back the code
    /// the extension or helper reported, and the REST layer turns it into a
    /// status and a body.
    fn connector_error(code: &str) -> ApiError {
        ApiError::from(HubError::new(code, "connector reported a failure"))
    }

    fn connector_status(code: &str) -> StatusCode {
        connector_error(code).status
    }

    #[test]
    fn every_registered_connector_code_reports_the_status_of_its_taxonomy_class() {
        for (legacy, expected_class) in crate::error_taxonomy::LEGACY_CODES {
            let error = connector_error(legacy);
            let class = classify(legacy).code;
            assert_eq!(
                class,
                *expected_class,
                "{legacy} no longer classifies as {}",
                expected_class.as_str()
            );
            assert_ne!(
                error.status,
                StatusCode::INTERNAL_SERVER_ERROR,
                "{legacy} ({}) is reported to clients as a local server fault",
                class.as_str()
            );
            let expected = connector_status_override(legacy).unwrap_or_else(|| class.http_status());
            assert_eq!(
                error.status,
                expected,
                "{legacy} ({}) reports {} instead of {expected}",
                class.as_str(),
                error.status
            );
            // The published body must explain the status it arrives with.
            let body = error.body();
            assert_eq!(body["error"]["code"], *legacy);
            assert_eq!(body["taxonomy"]["code"], class.as_str());
        }
    }

    #[test]
    fn unclassified_connector_codes_blame_the_connector_not_the_bridge() {
        // An unrecognized code still came from a connector, so it is a bad
        // gateway rather than a fault of this server.
        for unknown in [
            "SOMETHING_BRAND_NEW",
            "COMPUTER_ERROR",
            "EXTENSION_ERROR",
            "COMPUTER_HELPER_FAILED",
            "EVALUATION_FAILED",
            "SCREENSHOT_FAILED",
        ] {
            assert_eq!(
                connector_status(unknown),
                StatusCode::BAD_GATEWAY,
                "{unknown} did not report a connector fault"
            );
        }
    }

    #[test]
    fn a_human_pause_is_locked_and_never_a_server_error() {
        // The live 0.11.0 regression: a human clicked the in-page Stop
        // button, and browser.control.start and page.observe both answered
        // HTTP 500. A held human pause is a lock only a human can release.
        let paused = connector_error("HUMAN_CONTROL_PAUSED");
        assert_eq!(paused.status, StatusCode::LOCKED);
        assert_eq!(paused.status.as_u16(), 423);
        let body = paused.body();
        assert_eq!(body["error"]["code"], "HUMAN_CONTROL_PAUSED");
        assert_eq!(body["taxonomy"]["code"], "needs_user");
        assert_eq!(body["taxonomy"]["retriable"], false);
        assert_eq!(body["taxonomy"]["recoveryHint"], "handback");

        let dialoged = connector_error("BLOCKED_BY_DIALOG");
        assert_eq!(dialoged.status, StatusCode::CONFLICT);
        assert_eq!(dialoged.body()["taxonomy"]["code"], "blocked_by_dialog");
    }

    #[test]
    fn every_deliberate_connector_status_override_is_still_narrower_than_its_class() {
        // A stale override would silently re-introduce a hand-maintained
        // second source of truth, so each one must still disagree with the
        // class it overrides.
        for (legacy, _) in crate::error_taxonomy::LEGACY_CODES {
            let Some(override_status) = connector_status_override(legacy) else {
                continue;
            };
            assert_ne!(
                override_status,
                classify(legacy).code.http_status(),
                "{legacy} overrides its taxonomy class with the same status"
            );
        }
        for (code, expected) in [
            ("BAD_COORDINATES", StatusCode::BAD_REQUEST),
            ("COMPUTER_PERMISSION_REQUIRED", StatusCode::FORBIDDEN),
            ("NO_PENDING_DIALOG", StatusCode::CONFLICT),
        ] {
            assert_eq!(connector_status_override(code), Some(expected));
            assert_eq!(connector_status(code), expected);
        }
    }

    #[test]
    fn connector_statuses_that_predate_the_taxonomy_contract_are_unchanged() {
        // Every status the hand-maintained match already answered correctly,
        // pinned so the taxonomy-derived rewrite cannot regress one.
        for timeout in ["COMMAND_TIMEOUT", "COMMAND_OUTCOME_UNKNOWN"] {
            assert_eq!(connector_status(timeout), StatusCode::GATEWAY_TIMEOUT);
        }
        for conflict in [
            "COMPUTER_STALE_FRAME",
            "COMPUTER_STALE_POINTER",
            "CONTROL_REQUIRED",
            "CONTROL_REVOKED",
            "STALE_CONTROL_SESSION",
            "STALE_CONTROL_TURN",
            "STALE_MOVE_SEQUENCE",
            "STALE_SNAPSHOT",
            "STALE_REF",
            "TARGET_CHANGED",
            "TARGET_MISSING",
            "TARGET_OCCLUDED",
            "NO_PENDING_DIALOG",
            "WAIT_TIMEOUT",
        ] {
            assert_eq!(
                connector_status(conflict),
                StatusCode::CONFLICT,
                "{conflict}"
            );
        }
        for bad_request in [
            "COMPUTER_INVALID_REQUEST",
            "BAD_TAB",
            "BAD_URL",
            "BAD_BUTTON",
            "BAD_CLICK_COUNT",
            "BAD_COORDINATES",
            "BAD_KEY",
            "BAD_MODIFIER",
        ] {
            assert_eq!(
                connector_status(bad_request),
                StatusCode::BAD_REQUEST,
                "{bad_request}"
            );
        }
        for forbidden in [
            "COMPUTER_PERMISSION_REQUIRED",
            "SITE_BLOCKED",
            "FULL_ACCESS_REQUIRED",
            "SENSITIVE_FIELD",
        ] {
            assert_eq!(
                connector_status(forbidden),
                StatusCode::FORBIDDEN,
                "{forbidden}"
            );
        }
        for unavailable in [
            "EXTENSION_OFFLINE",
            "EXTENSION_DISCONNECTED",
            "EXTENSION_OVERLOADED",
            "COMPUTER_OFFLINE",
            "COMPUTER_DISCONNECTED",
            "COMPUTER_OVERLOADED",
        ] {
            assert_eq!(
                connector_status(unavailable),
                StatusCode::SERVICE_UNAVAILABLE,
                "{unavailable}"
            );
        }
    }

    #[test]
    fn the_server_still_owns_the_status_of_the_errors_it_builds_itself() {
        // The connector path must not have moved any status the server
        // constructs for its own refusals.
        assert_eq!(
            ApiError::bad_request("junk").status,
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            ApiError::forbidden("HOST_REJECTED", "loopback only").status,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            ApiError::forbidden("CSRF_REJECTED", "bad session").status,
            StatusCode::FORBIDDEN
        );
        assert_eq!(interrupted_call_error().status, StatusCode::GATEWAY_TIMEOUT);
        for (status, code) in [
            (StatusCode::UNAUTHORIZED, "UNAUTHORIZED"),
            (StatusCode::CONFLICT, "CALL_ID_REUSED"),
            (StatusCode::CONFLICT, "CALL_IN_PROGRESS"),
            (StatusCode::NOT_FOUND, "NOT_FOUND"),
            (StatusCode::UNSUPPORTED_MEDIA_TYPE, "UNSUPPORTED_MEDIA_TYPE"),
            (StatusCode::PAYLOAD_TOO_LARGE, "BODY_TOO_LARGE"),
            (StatusCode::TOO_MANY_REQUESTS, "AUTH_BUSY"),
        ] {
            assert_eq!(ApiError::new(status, code, "server refusal").status, status);
        }
    }
}

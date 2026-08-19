use std::collections::{HashMap, VecDeque};
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

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
use time::OffsetDateTime;
use tokio::net::TcpListener;
use tokio::sync::{OwnedSemaphorePermit, RwLock, Semaphore, broadcast};
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

use crate::computer::{COMPUTER_HELPER_ORIGIN, COMPUTER_METHODS};
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
];

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
        let state = AppState::new(config.token, config.call_timeout, config.check_for_updates);
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
    hub: ExtensionHub,
    computer_hub: ExtensionHub,
    data: Arc<RwLock<StateData>>,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    events: broadcast::Sender<ServerEvent>,
    action_lock: Arc<tokio::sync::Mutex<()>>,
    update_lock: Arc<tokio::sync::Mutex<()>>,
    browser_auth_slots: Arc<Semaphore>,
    computer_auth_slots: Arc<Semaphore>,
}

impl AppState {
    fn new(token: String, call_timeout: Duration, check_for_updates: bool) -> Self {
        let (events, _) = broadcast::channel(256);
        let mut data = StateData::default();
        data.public.update = if check_for_updates {
            UpdateStatus::checking()
        } else {
            UpdateStatus::disabled()
        };
        Self {
            token: Arc::new(token),
            hub: ExtensionHub::new(call_timeout),
            computer_hub: ExtensionHub::computer(call_timeout),
            data: Arc::new(RwLock::new(data)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            events,
            action_lock: Arc::new(tokio::sync::Mutex::new(())),
            update_lock: Arc::new(tokio::sync::Mutex::new(())),
            browser_auth_slots: Arc::new(Semaphore::new(MAX_PROVISIONAL_CONNECTIONS)),
            computer_auth_slots: Arc::new(Semaphore::new(MAX_PROVISIONAL_CONNECTIONS)),
        }
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
}

impl From<HubError> for ApiError {
    fn from(error: HubError) -> Self {
        let status = match error.code.as_str() {
            code if code.ends_with("_OFFLINE")
                || code.ends_with("_DISCONNECTED")
                || code.ends_with("_OVERLOADED") =>
            {
                StatusCode::SERVICE_UNAVAILABLE
            }
            "COMMAND_TIMEOUT" | "COMMAND_OUTCOME_UNKNOWN" => StatusCode::GATEWAY_TIMEOUT,
            "COMPUTER_STALE_FRAME"
            | "COMPUTER_STALE_POINTER"
            | "CONTROL_REQUIRED"
            | "CONTROL_REVOKED"
            | "STALE_CONTROL_SESSION"
            | "STALE_CONTROL_TURN"
            | "STALE_MOVE_SEQUENCE"
            | "STALE_SNAPSHOT"
            | "STALE_REF"
            | "TARGET_CHANGED"
            | "TARGET_MISSING"
            | "TARGET_OCCLUDED" => StatusCode::CONFLICT,
            "COMPUTER_INVALID_REQUEST"
            | "BAD_TAB"
            | "BAD_URL"
            | "BAD_BUTTON"
            | "BAD_CLICK_COUNT"
            | "BAD_COORDINATES"
            | "BAD_KEY" => StatusCode::BAD_REQUEST,
            "COMPUTER_PERMISSION_REQUIRED"
            | "SITE_BLOCKED"
            | "FULL_ACCESS_REQUIRED"
            | "SENSITIVE_FIELD" => StatusCode::FORBIDDEN,
            code if code.starts_with("COMPUTER_") || code.starts_with("EXTENSION_") => {
                StatusCode::BAD_GATEWAY
            }
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self::new(status, error.code, error.message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(json!({ "ok": false, "error": { "code": self.code, "message": self.message } })),
        )
            .into_response()
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
        .layer(middleware::from_fn(loopback_and_security_headers))
        .with_state(state)
}

async fn loopback_and_security_headers(request: Request, next: Next) -> Response {
    let allowed = request
        .headers()
        .get(HOST)
        .and_then(|value| value.to_str().ok())
        .is_some_and(is_loopback_host);
    let mut response = if allowed {
        next.run(request).await
    } else {
        ApiError::new(
            StatusCode::BAD_REQUEST,
            "HOST_REJECTED",
            "Only loopback hosts are accepted",
        )
        .into_response()
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
    let result = state
        .perform_action(
            &method,
            body.get("params").cloned().unwrap_or_else(|| json!({})),
        )
        .await?;
    let public = state.public_state().await;
    Ok(Json(json!({ "ok": true, "result": result, "state": public })).into_response())
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
    let capabilities = message
        .get("capabilities")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|item| COMPUTER_METHODS.contains(item))
                .take(COMPUTER_METHODS.len())
                .map(|item| item.to_owned())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
        .into_iter()
        .filter(|_| compatible)
        .collect();
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
                Ok(None) => return,
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
        _ => {}
    }
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
        let (target_tab_id, browser_control) = {
            let data = self.data.read().await;
            (
                data.public.target_tab_id,
                data.public.browser_control.clone(),
            )
        };
        let mut params = sanitize_params(method, raw_params, target_tab_id)?;
        if method.starts_with("page.") && method != "page.observe" {
            bind_browser_control(&mut params, &browser_control);
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
            let target = self.data.read().await.public.target_tab_id;
            if let Some(tab_id) = target
                && let Err(error) = self
                    .refresh_observation_for(tab_id, Some(connection_id))
                    .await
            {
                self.log("page.observe", "warning", &error.message).await;
                self.bump("warning").await;
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
        let mut params = sanitize_computer_params(method, raw_params)?;
        let (computer, pointer_revision) = {
            let data = self.data.read().await;
            (
                data.public.computer.clone(),
                data.public
                    .computer_observation
                    .as_ref()
                    .map(|observation| observation.pointer.sequence),
            )
        };
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
            elements: snapshot
                .and_then(|item| item.get("elements"))
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(sanitize_element)
                        .take(250)
                        .collect()
                })
                .unwrap_or_default(),
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
        "page.click" | "page.clickAt" => 350,
        "page.fill" | "page.typeText" => 100,
        "page.select" => 200,
        "page.key" => 250,
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

fn sanitize_computer_params(method: &str, input: Value) -> Result<Value, ApiError> {
    let source = input.as_object().cloned().unwrap_or_default();
    if !COMPUTER_METHODS.contains(&method) {
        return Err(ApiError::bad_request("Unsupported computer action"));
    }
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
            let mut output = computer_frame_params(&source)?;
            output.insert("x".to_owned(), json!(computer_coordinate(&source, "x")?));
            output.insert("y".to_owned(), json!(computer_coordinate(&source, "y")?));
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
            let mut output = computer_frame_params(&source)?;
            for name in ["fromX", "fromY", "toX", "toY"] {
                output.insert(name.to_owned(), json!(computer_coordinate(&source, name)?));
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
            output.insert(
                "key".to_owned(),
                Value::String(required_string(source.get("key"), "key", 200)?),
            );
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
        "page.click" => ref_params(&source, with_tab()?, None),
        "page.fill" => ref_params(&source, with_tab()?, Some(("text", 10_000))),
        "page.select" => ref_params(&source, with_tab()?, Some(("value", 1_000))),
        "page.key" => {
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
                "key".to_owned(),
                Value::String(required_string(source.get("key"), "key", 80)?),
            );
            Ok(Value::Object(output))
        }
        "page.scroll" => {
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

    if method.starts_with("page.") && method != "page.observe" {
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

fn bind_browser_control(params: &mut Value, control: &Value) {
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
    for name in ["turn", "moveSequence"] {
        if !output.contains_key(name)
            && let Some(value) = control.get(name).and_then(Value::as_u64)
        {
            output.insert(name.to_owned(), json!(value));
        }
    }
}

fn ref_params(
    source: &Map<String, Value>,
    base: Value,
    extra: Option<(&str, usize)>,
) -> Result<Value, ApiError> {
    let mut output = object(base);
    output.insert(
        "ref".to_owned(),
        Value::String(required_string(source.get("ref"), "ref", 80)?),
    );
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

fn sanitize_element(value: &Value) -> Option<ElementInfo> {
    let reference = bounded(value.get("ref")?.as_str()?, 80);
    if reference.is_empty() {
        return None;
    }
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
    Ok(Some(Screenshot {
        bytes: Bytes::from(bytes),
        content_type,
        id: Uuid::new_v4().simple().to_string(),
        binding: String::new(),
        route: "",
    }))
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

fn is_loopback_host(host: &str) -> bool {
    let hostname = if host.starts_with('[') {
        host.strip_prefix('[')
            .and_then(|value| value.split_once(']'))
            .map(|(host, _)| host)
    } else {
        Some(host.split(':').next().unwrap_or(""))
    };
    matches!(hostname, Some("127.0.0.1" | "localhost" | "::1"))
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
        )
        .unwrap();
        assert_eq!(params["tabId"], 7);
        assert_eq!(params["button"], "right");
        assert!(
            sanitize_params(
                "page.clickAt",
                json!({ "tabId": 7, "generation": "g1", "x": 10, "y": -1 }),
                None
            )
            .is_err()
        );
        assert!(sanitize_params("page.evaluate", json!({ "tabId": 7 }), None).is_err());
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
        assert!(is_loopback_host("127.0.0.1:17373"));
        assert!(is_loopback_host("localhost:17373"));
        assert!(is_loopback_host("[::1]:17373"));
        assert!(!is_loopback_host("example.com:17373"));
    }

    #[test]
    fn expired_dashboard_session_cannot_mutate_even_with_its_old_csrf_token() {
        let state = AppState::new(create_token(), Duration::from_secs(1), false);
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
        )
        .unwrap();
        assert_eq!(invoke["elementRef"], "a1");
        let set_value = sanitize_computer_params(
            "computer.setValue",
            json!({ "frameId": "frame-1", "elementRef": "a2", "value": "hello" }),
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
}

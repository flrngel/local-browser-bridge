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
    AUTHORIZATION, CACHE_CONTROL, CONTENT_LENGTH, CONTENT_SECURITY_POLICY, CONTENT_TYPE, COOKIE,
    HOST, ORIGIN, REFERRER_POLICY, SET_COOKIE, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use futures_util::stream;
use futures_util::{SinkExt as _, StreamExt as _};
use include_dir::{Dir, include_dir};
use serde::Serialize;
use serde_json::{Map, Value, json};
use time::OffsetDateTime;
use tokio::net::TcpListener;
use tokio::sync::{RwLock, broadcast};
use tokio_stream::wrappers::BroadcastStream;
use uuid::Uuid;

use crate::VERSION;
use crate::hub::{ExtensionHub, HubError};
use crate::token::{create_token, tokens_equal};
use crate::update::{UpdateState, UpdateStatus, check_for_update};

const MAX_BODY_BYTES: usize = 128 * 1024;
const MAX_ACTIVITY: usize = 80;
const MAX_SCREENSHOT_BYTES: usize = 8 * 1024 * 1024;
const MAX_WS_MESSAGE_BYTES: usize = 8 * 1024 * 1024;
const PUBLIC_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/public");

pub const ACTION_METHODS: &[&str] = &[
    "status",
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
        result
    }
}

#[derive(Clone)]
struct AppState {
    token: Arc<String>,
    hub: ExtensionHub,
    data: Arc<RwLock<StateData>>,
    sessions: Arc<Mutex<HashMap<String, Session>>>,
    events: broadcast::Sender<ServerEvent>,
    action_lock: Arc<tokio::sync::Mutex<()>>,
    update_lock: Arc<tokio::sync::Mutex<()>>,
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
            data: Arc::new(RwLock::new(data)),
            sessions: Arc::new(Mutex::new(HashMap::new())),
            events,
            action_lock: Arc::new(tokio::sync::Mutex::new(())),
            update_lock: Arc::new(tokio::sync::Mutex::new(())),
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

    async fn public_state(&self) -> Value {
        let data = self.data.read().await;
        let mut value = serde_json::to_value(&data.public).unwrap_or_else(|_| json!({}));
        if let Some(observation) = value.get_mut("observation").and_then(Value::as_object_mut) {
            observation.insert(
                "screenshotUrl".to_owned(),
                if data.screenshot.is_some() {
                    Value::String(format!("/api/screenshot?revision={}", data.public.revision))
                } else {
                    Value::Null
                },
            );
        }
        value
    }

    fn ensure_session(&self, headers: &HeaderMap) -> (String, Option<String>) {
        let cookie_id = parse_cookie(headers, "lbb_session");
        let now = OffsetDateTime::now_utc().unix_timestamp();
        let mut sessions = self.sessions.lock().unwrap();
        if let Some(id) = cookie_id {
            if let Some(session) = sessions.get_mut(&id) {
                session.touched_at = now;
                return (session.csrf.clone(), None);
            }
        }

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
        let cookie = format!("lbb_session={id}; Path=/; HttpOnly; SameSite=Strict; Max-Age=43200");
        (csrf, Some(cookie))
    }

    fn assert_ui_mutation(&self, headers: &HeaderMap) -> Result<(), ApiError> {
        let id = parse_cookie(headers, "lbb_session")
            .ok_or_else(|| ApiError::forbidden("CSRF_REJECTED", "Invalid UI session"))?;
        let csrf = headers
            .get("x-csrf-token")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(&id)
            .ok_or_else(|| ApiError::forbidden("CSRF_REJECTED", "Invalid UI session"))?;
        if !tokens_equal(csrf, &session.csrf) {
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
}

#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PublicState {
    revision: u64,
    connected: bool,
    extension: Option<ExtensionInfo>,
    target_tab_id: Option<u64>,
    tabs: Vec<TabInfo>,
    observation: Option<Observation>,
    activity: VecDeque<Activity>,
    update: UpdateStatus,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExtensionInfo {
    version: String,
    browser: String,
    mode: String,
    capabilities: Vec<String>,
    connected_at: String,
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
        let status = if error.code == "EXTENSION_OFFLINE" {
            StatusCode::SERVICE_UNAVAILABLE
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
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
        .route("/api/events", get(api_events))
        .route("/api/action", post(api_action))
        .route("/api/update/check", post(api_update_check))
        .route("/api/v1/command", post(api_command))
        .route("/bridge", get(websocket_upgrade))
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
    Json(json!({ "ok": true, "extensionConnected": state.hub.connected(), "version": VERSION }))
}

async fn api_session(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let (csrf, cookie) = state.ensure_session(&headers);
    with_cookie(
        Json(json!({ "ok": true, "csrfToken": csrf })).into_response(),
        cookie,
    )
}

async fn api_state(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let (_, cookie) = state.ensure_session(&headers);
    let public = state.public_state().await;
    with_cookie(
        Json(json!({ "ok": true, "state": public })).into_response(),
        cookie,
    )
}

async fn api_screenshot(State(state): State<AppState>) -> Result<Response, ApiError> {
    let screenshot = state.data.read().await.screenshot.clone().ok_or_else(|| {
        ApiError::new(
            StatusCode::NOT_FOUND,
            "NO_SCREENSHOT",
            "No screenshot has been captured",
        )
    })?;
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
    let (_, cookie) = state.ensure_session(&headers);
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
    let response = Sse::new(initial.chain(updates))
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("heartbeat"),
        )
        .into_response();
    with_cookie(response, cookie)
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
    let supplied = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or("");
    if !tokens_equal(supplied, &state.token) {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "Bearer token required",
        ));
    }
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
    if !origin.starts_with("chrome-extension://") {
        return Err(ApiError::forbidden(
            "ORIGIN_REJECTED",
            "Extension Origin required",
        ));
    }
    let supplied = query.get("token").map(String::as_str).unwrap_or("");
    if !tokens_equal(supplied, &state.token) {
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "UNAUTHORIZED",
            "Invalid extension token",
        ));
    }
    Ok(ws
        .max_message_size(MAX_WS_MESSAGE_BYTES)
        .on_upgrade(move |socket| handle_websocket(socket, state))
        .into_response())
}

async fn handle_websocket(socket: WebSocket, state: AppState) {
    let (connection_id, mut outgoing) = state.hub.attach();
    {
        let mut data = state.data.write().await;
        data.public.connected = true;
    }
    state
        .log("bridge", "ok", "Browser extension connected")
        .await;
    state.bump("connection").await;

    let (mut socket_sender, mut socket_receiver) = socket.split();
    let writer = tokio::spawn(async move {
        while let Some(message) = outgoing.recv().await {
            let closing = matches!(message, Message::Close(_));
            if socket_sender.send(message).await.is_err() || closing {
                break;
            }
        }
    });

    while let Some(Ok(message)) = socket_receiver.next().await {
        match message {
            Message::Text(text) => {
                let Ok(message) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                match message.get("type").and_then(Value::as_str) {
                    Some("ping") => {
                        let _ = state.hub.send(json!({ "type": "pong" }));
                    }
                    Some("hello") => handle_hello(&state, &message).await,
                    Some("event") => handle_extension_event(&state, &message).await,
                    _ if message.get("id").and_then(Value::as_str).is_some() => {
                        state.hub.resolve(&message)
                    }
                    _ => {}
                }
            }
            Message::Ping(bytes) => {
                let _ = state.hub.send_message(Message::Pong(bytes));
            }
            Message::Close(_) => break,
            _ => {}
        }
    }

    writer.abort();
    if state.hub.detach(connection_id) {
        {
            let mut data = state.data.write().await;
            data.public.connected = false;
            data.public.extension = None;
            data.public.tabs.clear();
            data.public.target_tab_id = None;
            data.public.observation = None;
            data.screenshot = None;
        }
        state
            .log("bridge", "warning", "Browser extension disconnected")
            .await;
        state.bump("connection").await;
    }
}

async fn handle_hello(state: &AppState, message: &Value) {
    let extension = ExtensionInfo {
        version: bounded(
            message
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("unknown"),
            50,
        ),
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
                    .take(100)
                    .map(|item| bounded(item, 80))
                    .collect()
            })
            .unwrap_or_default(),
        connected_at: now_iso(),
    };
    state.data.write().await.public.extension = Some(extension);
    state.bump("hello").await;

    let state = state.clone();
    tokio::spawn(async move {
        let _guard = state.action_lock.lock().await;
        if let Err(error) = state.refresh_tabs().await {
            state.log("tabs.list", "warning", error.message).await;
            state.bump("warning").await;
        }
    });
}

async fn handle_extension_event(state: &AppState, message: &Value) {
    let name = message
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let data = message.get("data").cloned().unwrap_or_else(|| json!({}));
    match name {
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
            state
                .log("approval", if ok { "ok" } else { "error" }, detail)
                .await;
            state.bump("approval").await;
            if ok {
                let state = state.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(Duration::from_millis(300)).await;
                    let _guard = state.action_lock.lock().await;
                    let _ = state.refresh_tabs().await;
                    let target = state.data.read().await.public.target_tab_id;
                    if data.get("method").and_then(Value::as_str) != Some("tabs.close") {
                        if let Some(tab_id) = target {
                            let _ = state.refresh_observation(tab_id).await;
                        }
                    }
                });
            }
        }
        "approval.rejected" => {
            state
                .log("approval", "warning", "Human rejected the pending action")
                .await;
            state.bump("approval").await;
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

fn with_cookie(mut response: Response, cookie: Option<String>) -> Response {
    if let Some(cookie) = cookie.and_then(|value| HeaderValue::from_str(&value).ok()) {
        response.headers_mut().insert(SET_COOKIE, cookie);
    }
    response
}

impl AppState {
    async fn perform_action(&self, method: &str, raw_params: Value) -> Result<Value, ApiError> {
        if !ACTION_METHODS.contains(&method) {
            return Err(ApiError::bad_request("Unsupported action"));
        }
        let _guard = self.action_lock.lock().await;
        let target_tab_id = self.data.read().await.public.target_tab_id;
        let params = sanitize_params(method, raw_params, target_tab_id)?;

        if method == "tabs.list" {
            return self.refresh_tabs().await;
        }
        if method == "page.observe" {
            return self
                .refresh_observation(params["tabId"].as_u64().unwrap())
                .await;
        }

        let result = match self.hub.call(method, params.clone()).await {
            Ok(result) => result,
            Err(error) => {
                self.log(method, "error", &error.message).await;
                self.bump("error").await;
                return Err(error.into());
            }
        };

        {
            let mut data = self.data.write().await;
            if method == "tabs.activate" {
                data.public.target_tab_id = params.get("tabId").and_then(Value::as_u64);
            } else if method == "tabs.new" {
                if let Some(tab_id) = result.get("tabId").and_then(Value::as_u64) {
                    data.public.target_tab_id = Some(tab_id);
                }
            }
        }

        if result.get("status").and_then(Value::as_str) == Some("approval_required") {
            let risk = result
                .get("risk")
                .and_then(Value::as_str)
                .unwrap_or("Sensitive action");
            self.log(
                method,
                "approval",
                format!("{risk}; approve it in the extension popup"),
            )
            .await;
            self.bump("approval").await;
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
        self.log(method, "ok", log_message).await;

        if method.starts_with("tabs.") {
            if let Err(error) = self.refresh_tabs().await {
                self.log("tabs.list", "warning", &error.message).await;
                self.bump("warning").await;
            }
        }

        if let Some(delay) = observation_delay(method) {
            tokio::time::sleep(delay).await;
            let target = self.data.read().await.public.target_tab_id;
            if let Some(tab_id) = target {
                if let Err(error) = self.refresh_observation(tab_id).await {
                    self.log("page.observe", "warning", &error.message).await;
                    self.bump("warning").await;
                }
            }
        }

        Ok(result)
    }

    async fn refresh_tabs(&self) -> Result<Value, ApiError> {
        let result = self
            .hub
            .call("tabs.list", json!({}))
            .await
            .map_err(ApiError::from)?;
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
        self.bump("tabs").await;
        Ok(result)
    }

    async fn refresh_observation(&self, tab_id: u64) -> Result<Value, ApiError> {
        let result = self
            .hub
            .call("page.observe", json!({ "tabId": tab_id }))
            .await
            .map_err(ApiError::from)?;
        let screenshot = decode_screenshot(result.get("screenshot"))?;
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
        {
            let mut data = self.data.write().await;
            data.public.target_tab_id = Some(tab_id);
            data.public.observation = Some(observation);
            data.screenshot = screenshot;
        }
        self.log("page.observe", "ok", format!("Observed tab {tab_id}"))
            .await;
        self.bump("observation").await;
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

    match method {
        "status" | "tabs.list" | "tabs.new" => Ok(json!({})),
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
                "key".to_owned(),
                Value::String(required_string(source.get("key"), "key", 80)?),
            );
            Ok(Value::Object(output))
        }
        "page.scroll" => {
            let mut output = object(with_tab()?);
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

fn parse_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(COOKIE)
        .and_then(|value| value.to_str().ok())?
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find_map(|(key, value)| (key == name).then(|| value.to_owned()))
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
    fn validates_full_access_action_parameters() {
        let params = sanitize_params(
            "page.clickAt",
            json!({ "tabId": 7, "x": 10.5, "y": 20, "button": "right", "clickCount": 2 }),
            None,
        )
        .unwrap();
        assert_eq!(params["tabId"], 7);
        assert_eq!(params["button"], "right");
        assert!(
            sanitize_params(
                "page.clickAt",
                json!({ "tabId": 7, "x": 10, "y": -1 }),
                None
            )
            .is_err()
        );
        assert!(sanitize_params("page.evaluate", json!({ "tabId": 7 }), None).is_err());
    }

    #[test]
    fn enforces_loopback_host_boundary() {
        assert!(is_loopback_host("127.0.0.1:17373"));
        assert!(is_loopback_host("localhost:17373"));
        assert!(is_loopback_host("[::1]:17373"));
        assert!(!is_loopback_host("example.com:17373"));
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
}

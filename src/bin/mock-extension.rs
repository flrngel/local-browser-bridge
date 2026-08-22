use std::env;
use std::path::PathBuf;

use futures_util::{SinkExt as _, StreamExt as _};
use local_browser_bridge::ws_auth::{
    AUTH_TIMEOUT, BROWSER_CONNECTOR, ClientHello, MAX_AUTH_MESSAGE_BYTES, MAX_AUTH_MESSAGES,
};
use local_browser_bridge::{PROTOCOL_VERSION, VERSION, default_token_path, load_or_create_token};
use serde_json::{Value, json};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

const PIXEL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let port = env::var("LBB_PORT").unwrap_or_else(|_| "17373".to_owned());
    let token_path = env::var_os("LBB_TOKEN_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_token_path);
    let token = match env::var("LBB_TOKEN") {
        Ok(value) if !value.trim().is_empty() => value.trim().to_owned(),
        _ => load_or_create_token(&token_path).await?,
    };
    let url = format!("ws://127.0.0.1:{port}/bridge");
    let mut request = url.into_client_request()?;
    request.headers_mut().insert(
        "Origin",
        "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".parse()?,
    );
    let (mut socket, _) = connect_async(request).await?;
    let deadline = tokio::time::Instant::now() + AUTH_TIMEOUT;
    let client = ClientHello::new(BROWSER_CONNECTOR)?;
    socket
        .send(Message::Text(client.envelope().to_string().into()))
        .await?;
    let mut authenticated_session: Option<String> = None;
    let mut welcome = None;
    for _ in 0..MAX_AUTH_MESSAGES {
        let message = tokio::time::timeout_at(deadline, socket.next())
            .await
            .map_err(|_| "Server authentication timed out")?
            .ok_or("Server closed during authentication")??;
        match message {
            Message::Text(text) => {
                if text.len() > MAX_AUTH_MESSAGE_BYTES {
                    return Err("Server authentication message was too large".into());
                }
                let message: Value = serde_json::from_str(text.as_str())?;
                if let Some(session_id) = authenticated_session.as_ref() {
                    if message.get("type").and_then(Value::as_str) != Some("welcome")
                        || message.get("sessionId").and_then(Value::as_str)
                            != Some(session_id.as_str())
                    {
                        return Err("Authenticated server welcome did not match".into());
                    }
                    welcome = Some(message);
                    break;
                }
                let (session_id, response) = client.answer_challenge(&token, &message)?;
                socket
                    .send(Message::Text(response.to_string().into()))
                    .await?;
                authenticated_session = Some(session_id.to_string());
            }
            Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await?,
            Message::Pong(_) => {}
            Message::Close(_) => return Err("Server closed during authentication".into()),
            _ => return Err("Server sent a non-text authentication message".into()),
        }
    }
    let welcome = welcome.ok_or("Server authentication message limit exceeded")?;
    let session_id = welcome
        .get("sessionId")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or("Server welcome has no sessionId")?
        .to_owned();
    if welcome.get("protocolVersion").and_then(Value::as_u64) != Some(PROTOCOL_VERSION)
        || welcome.get("serverVersion").and_then(Value::as_str) != Some(VERSION)
        || welcome.get("connector").and_then(Value::as_str) != Some("browser-extension")
    {
        return Err("Server protocol welcome is incompatible".into());
    }
    socket
        .send(Message::Text(
            json!({
                "type": "hello",
                "version": VERSION,
                "protocolVersion": PROTOCOL_VERSION,
                "sessionId": session_id,
                "controllerId": "rust-mock-controller",
                "connectionId": uuid::Uuid::new_v4().to_string(),
                "browser": "Mock Chromium",
                "mode": "full-access",
                "capabilities": ["status", "browser.control.start", "browser.control.status", "browser.control.stop", "tabs.list", "tabs.activate", "tabs.new", "tabs.close", "page.observe", "page.navigate", "page.back", "page.forward", "page.reload", "page.click", "page.fill", "page.select", "page.key", "page.scroll", "page.clickAt", "page.typeText", "page.evaluate"]
            })
            .to_string()
            .into(),
        ))
        .await?;
    loop {
        let Some(message) = socket.next().await else {
            return Err("Server closed before helloAck".into());
        };
        match message? {
            Message::Text(text) => {
                let message: Value = serde_json::from_str(text.as_str())?;
                if message.get("type").and_then(Value::as_str) == Some("helloAck") {
                    if message.get("ok").and_then(Value::as_bool) != Some(true)
                        || message.get("protocolVersion").and_then(Value::as_u64)
                            != Some(PROTOCOL_VERSION)
                        || message.get("sessionId").and_then(Value::as_str)
                            != Some(session_id.as_str())
                    {
                        return Err("Server rejected the mock extension handshake".into());
                    }
                    break;
                }
            }
            Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await?,
            Message::Close(_) => return Ok(()),
            _ => {}
        }
    }
    let (mut writer, mut reader) = socket.split();
    println!("Rust mock extension connected to ws://127.0.0.1:{port}/bridge");

    let mut display_name = String::new();
    let mut selected_color = "green".to_owned();
    let mut last_sequence = 0_u64;
    while let Some(message) = reader.next().await {
        let message = message?;
        let Message::Text(text) = message else {
            continue;
        };
        let parsed: Value = serde_json::from_str(text.as_str())?;
        if parsed.get("protocolVersion").and_then(Value::as_u64) != Some(PROTOCOL_VERSION)
            || parsed.get("sessionId").and_then(Value::as_str) != Some(session_id.as_str())
        {
            return Err("Server message used a mismatched protocol session".into());
        }
        if parsed.get("type").and_then(Value::as_str) == Some("pong") {
            continue;
        }
        if parsed.get("type").and_then(Value::as_str) == Some("ping") {
            writer
                .send(Message::Text(
                    json!({
                        "type": "pong",
                        "protocolVersion": PROTOCOL_VERSION,
                        "sessionId": session_id
                    })
                    .to_string()
                    .into(),
                ))
                .await?;
            continue;
        }
        let sequence = parsed
            .get("sequence")
            .and_then(Value::as_u64)
            .ok_or("Command has no sequence")?;
        if sequence <= last_sequence {
            return Err("Command sequence is stale".into());
        }
        last_sequence = sequence;
        let Some(id) = parsed.get("id").and_then(Value::as_str) else {
            continue;
        };
        let method = parsed.get("method").and_then(Value::as_str).unwrap_or("");
        let params = parsed.get("params").cloned().unwrap_or_else(|| json!({}));
        let result = handle(method, &params, &mut display_name, &mut selected_color);
        writer
            .send(Message::Text(
                json!({
                    "id": id, "type": "result", "ok": true, "result": result,
                    "protocolVersion": PROTOCOL_VERSION,
                    "sessionId": session_id,
                    "sequence": sequence
                })
                .to_string()
                .into(),
            ))
            .await?;
    }
    Ok(())
}

fn handle(
    method: &str,
    params: &Value,
    display_name: &mut String,
    selected_color: &mut String,
) -> Value {
    match method {
        "status" => {
            json!({ "connected": true, "mock": true, "fullAccess": true, "control": mock_control() })
        }
        "browser.control.start" | "browser.control.status" => mock_control(),
        "browser.control.stop" => json!({
            "active": false,
            "revocation": {
                "sessionId": "mock-control-session",
                "reason": "released_by_client",
                "at": 1,
                "requiresExplicitStart": false
            }
        }),
        "tabs.list" => json!({
            "activeTabId": 101,
            "tabs": [{ "id": 101, "title": "Mock target tab", "url": "http://127.0.0.1:9000/demo", "active": true }]
        }),
        "tabs.activate" => json!({ "tabId": params["tabId"], "active": true }),
        "tabs.new" => json!({ "tabId": 102 }),
        "tabs.close" => json!({ "closed": true, "tabId": params["tabId"] }),
        "page.observe" => observation(display_name, selected_color),
        "page.fill" => {
            *display_name = params
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            json!({ "filled": true })
        }
        "page.select" => {
            *selected_color = params
                .get("value")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_owned();
            json!({ "selected": selected_color })
        }
        "page.click" => json!({ "clicked": true, "trusted": true }),
        "page.navigate" => json!({ "tabId": params["tabId"], "url": params["url"] }),
        "page.back" | "page.forward" | "page.reload" => json!({ "ok": true }),
        "page.key" => json!({ "pressed": params["key"] }),
        "page.scroll" => json!({ "x": params["deltaX"], "y": params["deltaY"] }),
        "page.clickAt" => json!({
            "clicked": true, "trusted": true, "x": params["x"], "y": params["y"],
            "button": params["button"], "clickCount": params["clickCount"]
        }),
        "page.typeText" => {
            json!({ "typed": true, "length": params["text"].as_str().map(str::len).unwrap_or(0) })
        }
        "page.evaluate" => {
            json!({ "type": "string", "value": format!("mock:{}", params["expression"].as_str().unwrap_or("")) })
        }
        _ => json!({ "ok": true }),
    }
}

fn observation(display_name: &str, selected_color: &str) -> Value {
    json!({
        "control": mock_control(),
        "snapshot": {
            "generation": format!("mock-{}", time::OffsetDateTime::now_utc().unix_timestamp_nanos()),
            "title": "Mock target tab",
            "url": "http://127.0.0.1:9000/demo",
            "viewport": { "width": 1280, "height": 720, "devicePixelRatio": 1 },
            "scroll": { "x": 0, "y": 0, "maxY": 400 },
            "bodyText": format!("Browser Bridge mock target. Display name: {}. Favorite color: {selected_color}.", if display_name.is_empty() { "empty" } else { display_name }),
            "elements": [
                { "ref": "e1", "role": "textbox", "name": "Display name", "type": "text", "disabled": false, "inViewport": true },
                { "ref": "e2", "role": "select", "name": "Favorite color", "type": "", "disabled": false, "inViewport": true },
                { "ref": "e3", "role": "button", "name": "Show greeting", "type": "submit", "disabled": false, "inViewport": true }
            ]
        },
        "screenshot": PIXEL
    })
}

fn mock_control() -> Value {
    json!({
        "active": true,
        "sessionId": "mock-control-session",
        "tabId": 101,
        "startedAt": 1,
        "expiresAt": 9_999_999_999_999_u64,
        "lastHeartbeatAt": 1,
        "turn": 1,
        "moveSequence": 0,
        "cursor": { "x": 0, "y": 0, "visible": false, "updatedAt": 1 }
    })
}

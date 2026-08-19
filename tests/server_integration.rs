use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use local_browser_bridge::computer::COMPUTER_HELPER_ORIGIN;
use local_browser_bridge::ws_auth::{
    AUTH_TIMEOUT, BROWSER_CONNECTOR, COMPUTER_CONNECTOR, ClientHello, MAX_AUTH_MESSAGE_BYTES,
    MAX_AUTH_MESSAGES,
};
use local_browser_bridge::{BridgeServer, PROTOCOL_VERSION, ServerConfig, VERSION, create_token};
use reqwest::Client;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const PIXEL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

type TestSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn authenticate_test_connector(
    socket: &mut TestSocket,
    token: &str,
    connector: &'static str,
) -> Value {
    let deadline = tokio::time::Instant::now() + AUTH_TIMEOUT;
    let client = ClientHello::new(connector).unwrap();
    socket
        .send(Message::Text(client.envelope().to_string().into()))
        .await
        .unwrap();
    let mut authenticated_session: Option<String> = None;
    for _ in 0..MAX_AUTH_MESSAGES {
        let message = tokio::time::timeout_at(deadline, socket.next())
            .await
            .expect("authentication deadline")
            .expect("authentication socket open")
            .expect("authentication message");
        match message {
            Message::Text(text) => {
                assert!(text.len() <= MAX_AUTH_MESSAGE_BYTES);
                let message: Value = serde_json::from_str(text.as_str()).unwrap();
                if let Some(session_id) = authenticated_session.as_ref() {
                    assert_eq!(message["type"], "welcome");
                    assert_eq!(message["sessionId"], session_id.as_str());
                    return message;
                }
                let (session_id, response) = client.answer_challenge(token, &message).unwrap();
                socket
                    .send(Message::Text(response.to_string().into()))
                    .await
                    .unwrap();
                authenticated_session = Some(session_id.to_string());
            }
            Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await.unwrap(),
            other => panic!("unexpected authentication message: {other:?}"),
        }
    }
    panic!("authentication message limit exceeded")
}

async fn start_server(token: &str) -> (String, oneshot::Sender<()>, JoinHandle<()>) {
    let mut config = ServerConfig::new(0, token);
    config.call_timeout = Duration::from_secs(1);
    config.check_for_updates = false;
    let server = BridgeServer::bind(config).await.unwrap();
    let address = server.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    let handle = tokio::spawn(async move {
        server
            .serve(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });
    (
        format!("http://127.0.0.1:{}", address.port()),
        shutdown_tx,
        handle,
    )
}

async fn connect_fake_extension(base_url: &str, token: &str) -> JoinHandle<()> {
    let mut request = format!("{}/bridge", base_url.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "Origin",
        "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .parse()
            .unwrap(),
    );
    let (mut socket, _) = connect_async(request).await.unwrap();
    let welcome = authenticate_test_connector(&mut socket, token, BROWSER_CONNECTOR).await;
    let session_id = welcome["sessionId"].as_str().unwrap().to_owned();
    socket
        .send(Message::Text(
            json!({
                "type": "hello", "version": VERSION, "protocolVersion": PROTOCOL_VERSION,
                "sessionId": session_id, "controllerId": "fixture-controller", "connectionId": "fixture-connection",
                "browser": "Test Chrome", "mode": "full-access",
                "capabilities": ["status", "browser.control.start", "browser.control.status", "browser.control.stop", "tabs.list", "tabs.activate", "page.observe", "page.evaluate", "page.clickAt", "page.typeText"]
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    tokio::spawn(async move {
        let (mut writer, mut reader) = socket.split();
        while let Some(Ok(Message::Text(text))) = reader.next().await {
            let message: Value = serde_json::from_str(text.as_str()).unwrap();
            let Some(id) = message.get("id").and_then(Value::as_str) else {
                continue;
            };
            let result = match message.get("method").and_then(Value::as_str).unwrap_or("") {
                "tabs.list" => json!({
                    "activeTabId": 7,
                    "tabs": [{ "id": 7, "title": "Test tab", "url": "https://example.test/", "active": true }]
                }),
                "tabs.activate" => json!({ "tabId": 7, "active": true }),
                "page.observe" => json!({
                    "screenshot": PIXEL,
                    "control": {
                        "active": true,
                        "sessionId": "control-fixture",
                        "tabId": 7,
                        "startedAt": 1,
                        "expiresAt": 9999999999999_u64,
                        "lastHeartbeatAt": 2,
                        "turn": 3,
                        "moveSequence": 5,
                        "cursor": { "x": 10, "y": 20, "visible": true, "updatedAt": 2 }
                    },
                    "snapshot": {
                        "generation": "g1", "title": "Test tab", "url": "https://example.test/",
                        "bodyText": "Hello from the target page", "viewport": { "width": 800, "height": 600 },
                        "scroll": { "x": 0, "y": 0, "maxY": 0 },
                        "elements": [{ "ref": "e1", "role": "button", "name": "Continue", "type": "submit", "disabled": false, "inViewport": true }]
                    }
                }),
                "page.evaluate" => json!({
                    "type": "string",
                    "value": "Eval works",
                    "receivedControlSessionId": message["params"]["controlSessionId"],
                    "receivedTurn": message["params"]["turn"],
                    "receivedMoveSequence": message["params"]["moveSequence"]
                }),
                _ => json!({ "ok": true }),
            };
            writer
                .send(Message::Text(
                    json!({
                        "id": id, "type": "result", "ok": true, "result": result,
                        "protocolVersion": message["protocolVersion"],
                        "sessionId": message["sessionId"],
                        "sequence": message["sequence"]
                    })
                    .to_string()
                    .into(),
                ))
                .await
                .unwrap();
        }
    })
}

async fn connect_incompatible_extension(base_url: &str, token: &str) -> JoinHandle<()> {
    let mut request = format!("{}/bridge", base_url.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "Origin",
        "chrome-extension://bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
            .parse()
            .unwrap(),
    );
    let (mut socket, _) = connect_async(request).await.unwrap();
    let welcome = authenticate_test_connector(&mut socket, token, BROWSER_CONNECTOR).await;
    socket
        .send(Message::Text(
            json!({
                "type": "hello",
                "version": "9.9.9",
                "protocolVersion": PROTOCOL_VERSION,
                "sessionId": welcome["sessionId"],
                "controllerId": "incompatible-controller",
                "connectionId": "incompatible-connection",
                "browser": "Test Chrome",
                "mode": "full-access",
                "capabilities": ["tabs.list"]
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    tokio::spawn(async move {
        let (_, mut reader) = socket.split();
        while reader.next().await.is_some() {}
    })
}

async fn connect_fake_computer(base_url: &str, token: &str, version: &str) -> JoinHandle<()> {
    let mut request = format!("{}/computer", base_url.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", COMPUTER_HELPER_ORIGIN.parse().unwrap());
    let (mut socket, _) = connect_async(request).await.unwrap();
    let welcome = authenticate_test_connector(&mut socket, token, COMPUTER_CONNECTOR).await;
    let version = version.to_owned();
    let session_id = welcome["sessionId"].as_str().unwrap().to_owned();
    socket
        .send(Message::Text(
            json!({
                "type": "hello",
                "version": version,
                "protocolVersion": PROTOCOL_VERSION,
                "sessionId": session_id,
                "platform": "test-os",
                "architecture": "test-arch",
                "backend": "test-capture+test-input",
                "inputReady": true,
                "semanticReady": true,
                "capabilities": [
                    "computer.status", "computer.observe", "computer.move", "computer.click", "computer.invoke", "computer.setValue",
                    "computer.drag", "computer.scroll", "computer.typeText", "computer.key",
                    "computer.share.start", "computer.share.status", "computer.share.stop",
                    "computer.shell"
                ]
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    tokio::spawn(async move {
        let (mut writer, mut reader) = socket.split();
        let mut frame_number = 0_u64;
        let mut current_frame = String::new();
        while let Some(Ok(Message::Text(text))) = reader.next().await {
            let message: Value = serde_json::from_str(text.as_str()).unwrap();
            let Some(id) = message.get("id").and_then(Value::as_str) else {
                continue;
            };
            let method = message.get("method").and_then(Value::as_str).unwrap_or("");
            let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
            let mut response = match method {
                "computer.status" => json!({
                    "id": id, "type": "result", "ok": true,
                    "result": { "inputReady": true, "displayCount": 1, "frameReady": !current_frame.is_empty() }
                }),
                "computer.observe" => {
                    frame_number += 1;
                    current_frame = format!("frame-{frame_number}");
                    json!({
                        "id": id, "type": "result", "ok": true,
                        "result": {
                            "screenshot": PIXEL,
                            "frame": {
                                "id": current_frame,
                                "capturedAt": "2026-08-18T00:00:00Z",
                                "displayId": "display-1",
                                "displayIndex": 0,
                                "displayName": "Test display",
                                "imageWidth": 1,
                                "imageHeight": 1,
                                "screenX": 0,
                                "screenY": 0,
                                "screenWidth": 1,
                                "screenHeight": 1,
                                "scaleFactor": 1.0,
                                "rotation": 0.0,
                                "pointer": {
                                    "id": "fixture-cursor",
                                    "visible": false,
                                    "windowId": null,
                                    "imageX": null,
                                    "imageY": null,
                                    "screenX": null,
                                    "screenY": null,
                                    "headingDegrees": 45.0,
                                    "action": "idle",
                                    "pressed": false,
                                    "sequence": 0,
                                    "updatedAt": "2026-08-18T00:00:00Z",
                                    "coordinateSpace": "image-pixels",
                                    "style": { "theme": "fixture", "fill": "#26C6FF", "outline": "#FFFFFF", "logicalSize": 42, "hotspot": "tip" }
                                }
                            }
                        }
                    })
                }
                "computer.click" if params["frameId"] != current_frame => json!({
                    "id": id, "type": "result", "ok": false,
                    "error": { "code": "COMPUTER_STALE_FRAME", "message": "Observe again" }
                }),
                "computer.click" => json!({
                    "id": id, "type": "result", "ok": true,
                    "result": {
                        "x": params["x"], "y": params["y"], "clickCount": params["clickCount"],
                        "expectedPointerRevision": params["expectedPointerRevision"]
                    }
                }),
                _ => json!({
                    "id": id, "type": "result", "ok": false,
                    "error": { "code": "COMPUTER_UNSUPPORTED_ACTION", "message": "Unsupported test action" }
                }),
            };
            if let Some(object) = response.as_object_mut() {
                object.insert(
                    "protocolVersion".to_owned(),
                    message["protocolVersion"].clone(),
                );
                object.insert("sessionId".to_owned(), message["sessionId"].clone());
                object.insert("sequence".to_owned(), message["sequence"].clone());
            }
            writer
                .send(Message::Text(response.to_string().into()))
                .await
                .unwrap();
        }
    })
}

async fn wait_for_tabs(client: &Client, base_url: &str, token: &str) -> Value {
    for _ in 0..100 {
        let state: Value = client
            .get(format!("{base_url}/api/state"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if state["state"]["tabs"]
            .as_array()
            .is_some_and(|tabs| !tabs.is_empty())
        {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("tabs did not arrive");
}

async fn wait_for_computer(client: &Client, base_url: &str, token: &str) -> Value {
    for _ in 0..100 {
        let state: Value = client
            .get(format!("{base_url}/api/state"))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if state["state"]["computer"]
            .get("version")
            .and_then(Value::as_str)
            .is_some()
        {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("computer helper hello did not arrive");
}

#[tokio::test]
async fn serves_embedded_ui_and_defensive_headers() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let response = reqwest::get(&base_url).await.unwrap();
    assert_eq!(response.status(), 200);
    assert!(
        response.headers()["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("frame-ancestors 'none'")
    );
    assert!(
        response
            .headers()
            .get("access-control-allow-origin")
            .is_none()
    );
    assert!(response.text().await.unwrap().contains("Browser Bridge"));
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn protects_dashboard_state_events_and_captures_with_token_bootstrapped_sessions() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let client = Client::new();

    for path in [
        "/api/session",
        "/api/state",
        "/api/events",
        "/api/screenshot",
        "/api/computer/screenshot",
    ] {
        let response = client
            .get(format!("{base_url}{path}"))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), 401, "{path} must require authentication");
    }

    let session = client
        .get(format!("{base_url}/api/session"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(session.status(), 200);
    assert!(session.headers().get("set-cookie").is_none());
    let session: Value = session.json().await.unwrap();
    let session_token = session["sessionToken"].as_str().unwrap();
    let resumed: Value = client
        .get(format!("{base_url}/api/session"))
        .header("Authorization", format!("Session {session_token}"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(resumed["sessionToken"], session_token);
    assert_eq!(resumed["csrfToken"], session["csrfToken"]);
    let state = client
        .get(format!("{base_url}/api/state"))
        .header("Authorization", format!("Session {session_token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(state.status(), 200);

    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn refuses_to_bind_with_empty_malformed_or_weak_tokens() {
    for token in [
        "",
        "   ",
        "not-a-token",
        "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",
    ] {
        let mut config = ServerConfig::new(0, token);
        config.check_for_updates = false;
        let error = match BridgeServer::bind(config).await {
            Ok(_) => panic!("invalid token unexpectedly bound a server"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
}

#[tokio::test]
async fn relays_commands_and_serves_observations() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let fake = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    let state = wait_for_tabs(&client, &base_url, &token).await;
    assert_eq!(state["state"]["extension"]["mode"], "full-access");

    let session_response = client
        .get(format!("{base_url}/api/session"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    let csrf: Value = session_response.json().await.unwrap();
    let session_token = csrf["sessionToken"].as_str().unwrap();
    let action: Value = client
        .post(format!("{base_url}/api/action"))
        .header("Authorization", format!("Session {session_token}"))
        .header("Origin", &base_url)
        .header("X-CSRF-Token", csrf["csrfToken"].as_str().unwrap())
        .json(&json!({ "method": "tabs.activate", "params": { "tabId": 7 } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(action["state"]["targetTabId"], 7);
    assert_eq!(
        action["state"]["observation"]["bodyText"],
        "Hello from the target page"
    );
    assert_eq!(
        action["state"]["observation"]["elements"][0]["type"],
        "submit"
    );

    let screenshot_url = action["state"]["observation"]["screenshotUrl"]
        .as_str()
        .unwrap();
    assert!(screenshot_url.starts_with("/api/screenshot?id="));
    let screenshot = client
        .get(format!("{base_url}{screenshot_url}"))
        .header("Authorization", format!("Session {session_token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(screenshot.status(), 200);
    assert_eq!(screenshot.headers()["content-type"], "image/png");
    assert!(screenshot.bytes().await.unwrap().len() > 10);

    let evaluated: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "page.evaluate", "params": { "tabId": 7, "expression": "document.title" } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(evaluated["result"]["value"], "Eval works");
    assert_eq!(
        evaluated["result"]["receivedControlSessionId"],
        "control-fixture"
    );
    assert_eq!(evaluated["result"]["receivedTurn"], 3);
    assert_eq!(evaluated["result"]["receivedMoveSequence"], 5);
    assert_eq!(evaluated["state"]["browserControl"]["active"], true);

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn rejects_an_extension_with_a_mismatched_package_protocol() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let fake = connect_incompatible_extension(&base_url, &token).await;
    let client = Client::new();
    let state = loop {
        let state: Value = client
            .get(format!("{base_url}/api/state"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if !state["state"]["extension"].is_null() {
            break state;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    };
    assert_eq!(state["state"]["connected"], false);
    assert_eq!(state["state"]["extension"]["compatible"], false);
    assert_eq!(state["state"]["extension"]["capabilities"], json!([]));

    let response = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "tabs.list", "params": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], "EXTENSION_PROTOCOL_MISMATCH");

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn relays_frame_bound_computer_actions_and_serves_desktop_capture() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let fake = connect_fake_computer(&base_url, &token, VERSION).await;
    let client = Client::new();
    let state = wait_for_computer(&client, &base_url, &token).await;
    assert_eq!(state["state"]["computerConnected"], true);
    assert_eq!(state["state"]["computer"]["inputReady"], true);
    assert_eq!(state["state"]["computer"]["semanticReady"], true);
    assert_eq!(
        state["state"]["computer"]["capabilities"]
            .as_array()
            .unwrap()
            .len(),
        13
    );
    assert!(
        !state["state"]["computer"]["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("computer.shell"))
    );

    let observed: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "computer.observe", "params": {} }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(observed["result"]["frameId"], "frame-1");
    assert_eq!(
        observed["state"]["computerObservation"]["displayName"],
        "Test display"
    );
    let first_screenshot_url = observed["state"]["computerObservation"]["screenshotUrl"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(first_screenshot_url.starts_with("/api/computer/screenshot?id="));

    let screenshot = client
        .get(format!("{base_url}{first_screenshot_url}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(screenshot.status(), 200);
    assert_eq!(screenshot.headers()["content-type"], "image/png");
    assert!(screenshot.bytes().await.unwrap().len() > 10);

    let newer_observation: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "computer.observe", "params": {} }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let newer_screenshot_url = newer_observation["state"]["computerObservation"]["screenshotUrl"]
        .as_str()
        .unwrap();
    assert_ne!(newer_screenshot_url, first_screenshot_url);
    let stale = client
        .get(format!("{base_url}{first_screenshot_url}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), 409);
    let current = client
        .get(format!("{base_url}{newer_screenshot_url}"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(current.status(), 200);

    let clicked: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "computer.click",
            "params": { "frameId": "frame-2", "x": 0, "y": 0, "button": "left", "clickCount": 1 }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(clicked["result"]["clickCount"], 1);
    assert_eq!(clicked["result"]["expectedPointerRevision"], 0);
    assert_eq!(
        clicked["state"]["computerObservation"]["frameId"],
        "frame-3"
    );

    let stale = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "computer.click",
            "params": { "frameId": "frame-1", "x": 0, "y": 0, "button": "left", "clickCount": 1 }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(stale.status(), 409);
    let stale: Value = stale.json().await.unwrap();
    assert_eq!(stale["error"]["code"], "COMPUTER_STALE_FRAME");

    let shell = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "computer.shell", "params": { "command": "whoami" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(shell.status(), 400);

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn blocks_mismatched_computer_helper_versions() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let fake = connect_fake_computer(&base_url, &token, "9.9.9").await;
    let client = Client::new();
    let state = wait_for_computer(&client, &base_url, &token).await;
    assert_eq!(state["state"]["computer"]["compatible"], false);
    assert_eq!(state["state"]["computer"]["capabilities"], json!([]));

    let response = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "computer.observe", "params": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["error"]["code"], "COMPUTER_PROTOCOL_MISMATCH");

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn provisional_sockets_are_bounded_and_cannot_evict_a_ready_extension() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let fake = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    assert_eq!(
        wait_for_tabs(&client, &base_url, &token).await["state"]["tabs"][0]["id"],
        7
    );

    let mut provisional = Vec::new();
    for _ in 0..4 {
        let mut request = format!("{}/bridge", base_url.replace("http", "ws"))
            .into_client_request()
            .unwrap();
        request.headers_mut().insert(
            "Origin",
            "chrome-extension://cccccccccccccccccccccccccccccccc"
                .parse()
                .unwrap(),
        );
        provisional.push(connect_async(request).await.unwrap().0);
    }

    let mut overflow = format!("{}/bridge", base_url.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    overflow.headers_mut().insert(
        "Origin",
        "chrome-extension://dddddddddddddddddddddddddddddddd"
            .parse()
            .unwrap(),
    );
    let error = connect_async(overflow).await.unwrap_err();
    let status = match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => response.status(),
        other => panic!("unexpected websocket error: {other}"),
    };
    assert_eq!(status.as_u16(), 429);
    assert_eq!(
        wait_for_tabs(&client, &base_url, &token).await["state"]["tabs"][0]["id"],
        7
    );

    for mut socket in provisional {
        socket.close(None).await.unwrap();
    }
    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn rejects_cross_origin_and_non_extension_clients() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let client = Client::new();
    let session_response = client
        .get(format!("{base_url}/api/session"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    let csrf: Value = session_response.json().await.unwrap();
    let session_token = csrf["sessionToken"].as_str().unwrap();

    let missing_authorization = client
        .post(format!("{base_url}/api/v1/command"))
        .header("Content-Type", "application/json")
        .body(r#"{"method":"tabs.list","params":{}}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(missing_authorization.status(), 401);

    let cross_origin_text = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .header("Origin", "https://evil.example")
        .header("Content-Type", "text/plain")
        .body(r#"{"method":"tabs.list","params":{}}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(cross_origin_text.status(), 403);

    let same_origin_text = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .header("Origin", &base_url)
        .header("Content-Type", "text/plain")
        .body(r#"{"method":"tabs.list","params":{}}"#)
        .send()
        .await
        .unwrap();
    assert_eq!(same_origin_text.status(), 415);

    let response = client
        .post(format!("{base_url}/api/action"))
        .header("Authorization", format!("Session {session_token}"))
        .header("Origin", "https://evil.example")
        .header("X-CSRF-Token", csrf["csrfToken"].as_str().unwrap())
        .json(&json!({ "method": "tabs.list", "params": {} }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 403);

    let request = format!("{}/bridge?token={token}", base_url.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    let error = connect_async(request).await.unwrap_err();
    let status = match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => response.status(),
        other => panic!("unexpected websocket error: {other}"),
    };
    assert_eq!(status.as_u16(), 403);

    let mut legacy_extension_token =
        format!("{}/bridge?token={token}", base_url.replace("http", "ws"))
            .into_client_request()
            .unwrap();
    legacy_extension_token.headers_mut().insert(
        "Origin",
        "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .parse()
            .unwrap(),
    );
    let error = connect_async(legacy_extension_token).await.unwrap_err();
    let status = match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => response.status(),
        other => panic!("unexpected websocket error: {other}"),
    };
    assert_eq!(status.as_u16(), 400);

    let mut token_free_extension = format!("{}/bridge", base_url.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    token_free_extension.headers_mut().insert(
        "Origin",
        "chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .parse()
            .unwrap(),
    );
    let (mut socket, _) = connect_async(token_free_extension).await.unwrap();
    let client_hello = ClientHello::new(BROWSER_CONNECTOR).unwrap();
    socket
        .send(Message::Text(client_hello.envelope().to_string().into()))
        .await
        .unwrap();
    let Message::Text(challenge) = socket.next().await.unwrap().unwrap() else {
        panic!("expected authentication challenge");
    };
    let challenge: Value = serde_json::from_str(challenge.as_str()).unwrap();
    assert!(
        client_hello
            .answer_challenge(&create_token(), &challenge)
            .is_err()
    );
    socket.close(None).await.unwrap();

    let mut wrong_origin = format!("{}/computer", base_url.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    wrong_origin.headers_mut().insert(
        "Origin",
        "chrome-extension://not-the-helper".parse().unwrap(),
    );
    let error = connect_async(wrong_origin).await.unwrap_err();
    let status = match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => response.status(),
        other => panic!("unexpected websocket error: {other}"),
    };
    assert_eq!(status.as_u16(), 403);

    let mut wrong_token = format!("{}/computer?token=wrong", base_url.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    wrong_token
        .headers_mut()
        .insert("Origin", COMPUTER_HELPER_ORIGIN.parse().unwrap());
    let error = connect_async(wrong_token).await.unwrap_err();
    let status = match error {
        tokio_tungstenite::tungstenite::Error::Http(response) => response.status(),
        other => panic!("unexpected websocket error: {other}"),
    };
    assert_eq!(status.as_u16(), 400);

    let mut missing_computer_token = format!("{}/computer", base_url.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    missing_computer_token
        .headers_mut()
        .insert("Origin", COMPUTER_HELPER_ORIGIN.parse().unwrap());
    let (mut computer_socket, _) = connect_async(missing_computer_token).await.unwrap();
    computer_socket.close(None).await.unwrap();

    let _ = shutdown.send(());
    handle.await.unwrap();
}

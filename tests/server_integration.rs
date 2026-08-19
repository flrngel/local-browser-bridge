use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use local_browser_bridge::computer::COMPUTER_HELPER_ORIGIN;
use local_browser_bridge::{BridgeServer, ServerConfig, VERSION, create_token};
use reqwest::Client;
use serde_json::{Value, json};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

const PIXEL: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII=";

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
    let mut request = format!("{}/bridge?token={token}", base_url.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    request.headers_mut().insert(
        "Origin",
        "chrome-extension://test-extension".parse().unwrap(),
    );
    let (socket, _) = connect_async(request).await.unwrap();
    tokio::spawn(async move {
        let (mut writer, mut reader) = socket.split();
        writer
            .send(Message::Text(
                json!({
                    "type": "hello", "version": "0.6.0-test", "browser": "Test Chrome", "mode": "full-access",
                    "capabilities": ["tabs.list", "page.observe", "page.evaluate", "page.clickAt", "page.typeText"]
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
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
                    "snapshot": {
                        "generation": "g1", "title": "Test tab", "url": "https://example.test/",
                        "bodyText": "Hello from the target page", "viewport": { "width": 800, "height": 600 },
                        "scroll": { "x": 0, "y": 0, "maxY": 0 },
                        "elements": [{ "ref": "e1", "role": "button", "name": "Continue", "type": "submit", "disabled": false, "inViewport": true }]
                    }
                }),
                "page.evaluate" => json!({ "type": "string", "value": "Eval works" }),
                _ => json!({ "ok": true }),
            };
            writer
                .send(Message::Text(
                    json!({ "id": id, "type": "result", "ok": true, "result": result })
                        .to_string()
                        .into(),
                ))
                .await
                .unwrap();
        }
    })
}

async fn connect_fake_computer(base_url: &str, token: &str, version: &str) -> JoinHandle<()> {
    let mut request = format!("{}/computer?token={token}", base_url.replace("http", "ws"))
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("Origin", COMPUTER_HELPER_ORIGIN.parse().unwrap());
    let (socket, _) = connect_async(request).await.unwrap();
    let version = version.to_owned();
    tokio::spawn(async move {
        let (mut writer, mut reader) = socket.split();
        writer
            .send(Message::Text(
                json!({
                    "type": "hello",
                    "version": version,
                    "platform": "test-os",
                    "architecture": "test-arch",
                    "backend": "test-capture+test-input",
                    "inputReady": true,
                    "semanticReady": true,
                    "capabilities": [
                        "computer.status", "computer.observe", "computer.move", "computer.click", "computer.invoke", "computer.setValue",
                        "computer.drag", "computer.scroll", "computer.typeText", "computer.key",
                        "computer.shell"
                    ]
                })
                .to_string()
                .into(),
            ))
            .await
            .unwrap();
        let mut frame_number = 0_u64;
        let mut current_frame = String::new();
        while let Some(Ok(Message::Text(text))) = reader.next().await {
            let message: Value = serde_json::from_str(text.as_str()).unwrap();
            let Some(id) = message.get("id").and_then(Value::as_str) else {
                continue;
            };
            let method = message.get("method").and_then(Value::as_str).unwrap_or("");
            let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
            let response = match method {
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
                                "rotation": 0.0
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
                    "result": { "x": params["x"], "y": params["y"], "clickCount": params["clickCount"] }
                }),
                _ => json!({
                    "id": id, "type": "result", "ok": false,
                    "error": { "code": "COMPUTER_UNSUPPORTED_ACTION", "message": "Unsupported test action" }
                }),
            };
            writer
                .send(Message::Text(response.to_string().into()))
                .await
                .unwrap();
        }
    })
}

async fn wait_for_tabs(client: &Client, base_url: &str) -> Value {
    for _ in 0..100 {
        let state: Value = client
            .get(format!("{base_url}/api/state"))
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

async fn wait_for_computer(client: &Client, base_url: &str) -> Value {
    for _ in 0..100 {
        let state: Value = client
            .get(format!("{base_url}/api/state"))
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
async fn relays_commands_and_serves_observations() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let fake = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    let state = wait_for_tabs(&client, &base_url).await;
    assert_eq!(state["state"]["extension"]["mode"], "full-access");

    let session_response = client
        .get(format!("{base_url}/api/session"))
        .send()
        .await
        .unwrap();
    let cookie = session_response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf: Value = session_response.json().await.unwrap();
    let action: Value = client
        .post(format!("{base_url}/api/action"))
        .header("Cookie", cookie)
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

    let screenshot = client
        .get(format!("{base_url}/api/screenshot"))
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
    let state = wait_for_computer(&client, &base_url).await;
    assert_eq!(state["state"]["computerConnected"], true);
    assert_eq!(state["state"]["computer"]["inputReady"], true);
    assert_eq!(state["state"]["computer"]["semanticReady"], true);
    assert_eq!(
        state["state"]["computer"]["capabilities"]
            .as_array()
            .unwrap()
            .len(),
        10
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
    assert!(
        observed["state"]["computerObservation"]["screenshotUrl"]
            .as_str()
            .unwrap()
            .starts_with("/api/computer/screenshot?revision=")
    );

    let screenshot = client
        .get(format!("{base_url}/api/computer/screenshot"))
        .send()
        .await
        .unwrap();
    assert_eq!(screenshot.status(), 200);
    assert_eq!(screenshot.headers()["content-type"], "image/png");
    assert!(screenshot.bytes().await.unwrap().len() > 10);

    let clicked: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "computer.click",
            "params": { "frameId": "frame-1", "x": 0, "y": 0, "button": "left", "clickCount": 1 }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(clicked["result"]["clickCount"], 1);
    assert_eq!(
        clicked["state"]["computerObservation"]["frameId"],
        "frame-2"
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
    let state = wait_for_computer(&client, &base_url).await;
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
    assert_eq!(body["error"]["code"], "COMPUTER_VERSION_MISMATCH");

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
        .send()
        .await
        .unwrap();
    let cookie = session_response.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let csrf: Value = session_response.json().await.unwrap();
    let response = client
        .post(format!("{base_url}/api/action"))
        .header("Cookie", cookie)
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

    let mut wrong_origin = format!("{}/computer?token={token}", base_url.replace("http", "ws"))
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
    assert_eq!(status.as_u16(), 401);

    let _ = shutdown.send(());
    handle.await.unwrap();
}

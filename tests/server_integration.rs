use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
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
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
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

async fn connect_fake_extension(
    base_url: &str,
    token: &str,
) -> (
    JoinHandle<()>,
    Arc<Mutex<Vec<String>>>,
    mpsc::UnboundedSender<(String, Value)>,
) {
    let (handle, relayed, events, _) =
        connect_fake_extension_with_dialog_race(base_url, token).await;
    (handle, relayed, events)
}

/// The same fixture plus the switch that reproduces the live 0.10.0 race: a
/// dialog that opens after the server already decided to observe, so the
/// relayed `page.observe` comes back `BLOCKED_BY_DIALOG` instead of a
/// snapshot. Flipping the returned flag arms it.
async fn connect_fake_extension_with_dialog_race(
    base_url: &str,
    token: &str,
) -> (
    JoinHandle<()>,
    Arc<Mutex<Vec<String>>>,
    mpsc::UnboundedSender<(String, Value)>,
    Arc<AtomicBool>,
) {
    let observe_blocked = Arc::new(AtomicBool::new(false));
    let observe_blocked_fixture = observe_blocked.clone();
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
                "capabilities": ["status", "browser.control.start", "browser.control.status", "browser.control.stop", "tabs.list", "tabs.activate", "page.observe", "page.evaluate", "page.click", "page.clickAt", "page.typeText", "page.waitFor", "page.hover", "page.batch", "page.handleDialog"]
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let relayed_methods = Arc::new(Mutex::new(Vec::new()));
    let relayed_methods_log = relayed_methods.clone();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<(String, Value)>();
    let event_session_id = session_id.clone();
    let handle = tokio::spawn(async move {
        let (mut writer, mut reader) = socket.split();
        let mut event_sequence = 0_u64;
        let mut events_open = true;
        loop {
            let message = tokio::select! {
                event = event_rx.recv(), if events_open => {
                    let Some((name, data)) = event else {
                        events_open = false;
                        continue;
                    };
                    event_sequence += 1;
                    let envelope = json!({
                        "type": "event",
                        "name": name,
                        "data": data,
                        "protocolVersion": PROTOCOL_VERSION,
                        "sessionId": event_session_id,
                        "eventSequence": event_sequence,
                    });
                    writer
                        .send(Message::Text(envelope.to_string().into()))
                        .await
                        .unwrap();
                    continue;
                }
                message = reader.next() => message,
            };
            let Some(Ok(Message::Text(text))) = message else {
                break;
            };
            let message: Value = serde_json::from_str(text.as_str()).unwrap();
            let Some(id) = message.get("id").and_then(Value::as_str) else {
                continue;
            };
            if message.get("type").and_then(Value::as_str) != Some("command") {
                continue;
            }
            let method = message.get("method").and_then(Value::as_str).unwrap_or("");
            relayed_methods_log.lock().unwrap().push(method.to_owned());
            let mut response = match method {
                "tabs.list" => json!({
                    "id": id, "type": "result", "ok": true,
                    "result": {
                        "activeTabId": 7,
                        "tabs": [{ "id": 7, "title": "Test tab", "url": "https://example.test/", "active": true }]
                    }
                }),
                "tabs.activate" => json!({
                    "id": id, "type": "result", "ok": true,
                    "result": { "tabId": 7, "active": true }
                }),
                // Tab 9 stands in for a tab a human paused with the in-page
                // Stop button: the extension refuses control until the human
                // resumes it, and no retry by the caller can change that.
                "browser.control.start" if message["params"]["tabId"] == json!(9) => json!({
                    "id": id, "type": "result", "ok": false,
                    "error": {
                        "code": "HUMAN_CONTROL_PAUSED",
                        "message": "a human paused agent control on this tab; ask them to resume it"
                    }
                }),
                // The extension refuses the observation itself when the
                // dialog opened after the server passed its own gate: the
                // renderer is frozen and the lease is intact, so the answer
                // is the non-fatal BLOCKED_BY_DIALOG, not a revocation.
                "page.observe" if observe_blocked_fixture.load(Ordering::SeqCst) => json!({
                    "id": id, "type": "result", "ok": false,
                    "error": {
                        "code": "BLOCKED_BY_DIALOG",
                        "message": "a JavaScript confirm dialog is blocking the controlled page during observation completion; resolve it with page.handleDialog first"
                    }
                }),
                // Tab 8 stands in for a page with a merged cross-origin frame:
                // same observation shape plus frame provenance, and a raw
                // element array over the server's 250-element publication
                // cap. The frame's elements come LAST, exactly as the
                // extension appends them after the top document.
                "page.observe" if message["params"]["tabId"] == json!(8) => {
                    let mut elements = vec![
                        json!({
                            "ref": "g2.e1", "role": "button", "name": "Top", "type": "submit",
                            "disabled": false, "inViewport": true,
                            "bounds": { "x": 1, "y": 2, "width": 3, "height": 4 }
                        }),
                        // A page cannot forge cross-origin provenance onto a
                        // top-document element.
                        json!({
                            "ref": "g2.e999", "role": "link", "name": "Forged",
                            "disabled": false, "inViewport": true, "crossOrigin": true
                        }),
                    ];
                    for index in 2..=300 {
                        elements.push(json!({
                            "ref": format!("g2.e{index}"), "role": "link", "name": format!("Top {index}"),
                            "disabled": false, "inViewport": true
                        }));
                    }
                    elements.push(json!({
                        "ref": "g2.f1.e1", "role": "button", "name": "Pay", "type": "submit",
                        "disabled": false, "inViewport": true,
                        "bounds": { "x": 125, "y": 69, "width": 40, "height": 20 },
                        "frameRef": "f1", "frameId": "6A1B",
                        "frameUrlOrigin": "https://pay.example.test", "crossOrigin": true
                    }));
                    for index in 2..=60 {
                        elements.push(json!({
                            "ref": format!("g2.f1.e{index}"), "role": "link", "name": format!("Pay {index}"),
                            "disabled": false, "inViewport": true,
                            "frameRef": "f1", "frameId": "6A1B",
                            "frameUrlOrigin": "https://pay.example.test", "crossOrigin": true
                        }));
                    }
                    json!({
                        "id": id, "type": "result", "ok": true,
                        "result": {
                            "screenshot": PIXEL,
                            "control": {
                                "active": true, "sessionId": "control-fixture", "tabId": 8,
                                "startedAt": 1, "expiresAt": 9999999999999_u64, "lastHeartbeatAt": 2,
                                "turn": 3, "moveSequence": 5,
                                "cursor": { "x": 10, "y": 20, "visible": true, "updatedAt": 2 }
                            },
                            "snapshot": {
                                "generation": "g2", "title": "Framed tab", "url": "https://example.test/framed",
                                "bodyText": "Checkout", "viewport": { "width": 800, "height": 600 },
                                "scroll": { "x": 0, "y": 0, "maxY": 0 },
                                "elements": elements,
                                "frames": [
                                    {
                                        "ref": "f1", "frameId": "6A1B", "urlOrigin": "https://pay.example.test",
                                        "crossOrigin": true, "depth": 1,
                                        "offset": { "x": 120, "y": 64 },
                                        "size": { "width": 380, "height": 220 },
                                        "elementCount": 60, "truncated": false
                                    },
                                    { "ref": "f99", "frameId": "bogus" }
                                ],
                                "frameSummary": {
                                    "supported": true, "mode": "cdp-auto-attach",
                                    "ownersSeen": 3, "attached": 1, "merged": 1, "elementsDropped": 0,
                                    "skipped": [
                                        { "urlOrigin": "https://ads.example.test", "reason": "blank_document" },
                                        { "urlOrigin": "https://same.example.test", "reason": "same_process_frame" }
                                    ]
                                }
                            }
                        }
                    })
                }
                "page.observe" => json!({
                    "id": id, "type": "result", "ok": true,
                    "result": {
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
                    }
                }),
                "page.evaluate" => json!({
                    "id": id, "type": "result", "ok": true,
                    "result": {
                        "type": "string",
                        "value": "Eval works",
                        "receivedControlSessionId": message["params"]["controlSessionId"],
                        "receivedTurn": message["params"]["turn"],
                        "receivedMoveSequence": message["params"]["moveSequence"]
                    }
                }),
                "page.click"
                    if message["params"]["modifiers"]
                        .as_array()
                        .is_some_and(|modifiers| !modifiers.is_empty()) =>
                {
                    json!({
                        "id": id, "type": "result", "ok": true,
                        "result": {
                            "clicked": true,
                            "receivedRef": message["params"]["ref"],
                            "receivedButton": message["params"]["button"],
                            "receivedClickCount": message["params"]["clickCount"],
                            "receivedModifiers": message["params"]["modifiers"]
                        }
                    })
                }
                // Frame-scoped refs relay verbatim and report the frame
                // taxonomy the extension would produce for that frame state.
                "page.click"
                    if message["params"]["ref"]
                        .as_str()
                        .is_some_and(|reference| reference.contains(".f")) =>
                {
                    let reference = message["params"]["ref"].as_str().unwrap_or("");
                    let (code, detail) = if reference.ends_with(".e2") {
                        ("STALE_FRAME_TREE", "the frame owner element moved")
                    } else if reference.ends_with(".e3") {
                        (
                            "FRAME_ACTION_UNSUPPORTED",
                            "page.click cannot act inside this frame",
                        )
                    } else {
                        ("FRAME_DETACHED", "the frame that holds the target changed")
                    };
                    json!({
                        "id": id, "type": "result", "ok": false,
                        "error": { "code": code, "message": format!("{detail} (ref {reference})") }
                    })
                }
                "page.click" => json!({
                    "id": id, "type": "result", "ok": false,
                    "error": { "code": "STALE_SNAPSHOT", "message": "observe the page again before acting" }
                }),
                "page.hover" => json!({
                    "id": id, "type": "result", "ok": true,
                    "result": {
                        "hovered": true,
                        "receivedRef": message["params"]["ref"],
                        "receivedGeneration": message["params"]["generation"],
                        "receivedControlSessionId": message["params"]["controlSessionId"]
                    }
                }),
                "page.waitFor" => json!({
                    "id": id, "type": "result", "ok": true,
                    "result": {
                        "satisfied": true,
                        "elapsedMs": 40,
                        "receivedTimeoutMs": message["params"]["timeoutMs"],
                        "receivedText": message["params"]["text"],
                        "receivedControlSessionId": message["params"]["controlSessionId"]
                    }
                }),
                "page.clickAt" => json!({
                    "id": id, "type": "result", "ok": true,
                    "result": {
                        "clicked": true,
                        "receivedX": message["params"]["x"],
                        "receivedY": message["params"]["y"],
                        "receivedButton": message["params"]["button"]
                    }
                }),
                "page.batch" => json!({
                    "id": id, "type": "result", "ok": true,
                    "result": {
                        "completed": 2,
                        "total": message["params"]["actions"].as_array().map(Vec::len).unwrap_or(0),
                        "failedIndex": 2,
                        "failedError": "STALE_SNAPSHOT: observe the page again before acting",
                        "perStep": [
                            { "method": "page.fill", "ok": true },
                            { "method": "page.key", "ok": true },
                            { "method": "page.click", "ok": false, "error": "STALE_SNAPSHOT: observe the page again before acting" }
                        ],
                        "receivedGeneration": message["params"]["generation"],
                        "receivedActions": message["params"]["actions"],
                        "receivedControlSessionId": message["params"]["controlSessionId"],
                        "receivedTurn": message["params"]["turn"],
                        "receivedMoveSequence": message["params"]["moveSequence"]
                    }
                }),
                "page.handleDialog" => json!({
                    "id": id, "type": "result", "ok": true,
                    "result": {
                        "handled": true,
                        "receivedAccept": message["params"]["accept"],
                        "receivedPromptText": message["params"]["promptText"],
                        "receivedControlSessionId": message["params"]["controlSessionId"],
                        "receivedTurn": message["params"]["turn"],
                        "receivedMoveSequence": message["params"]["moveSequence"]
                    }
                }),
                "page.typeText" => {
                    // Stays in flight long enough for a concurrent duplicate
                    // callId to be observed as CALL_IN_PROGRESS.
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    json!({
                        "id": id, "type": "result", "ok": true,
                        "result": { "typed": true }
                    })
                }
                _ => json!({
                    "id": id, "type": "result", "ok": true,
                    "result": { "ok": true }
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
    });
    (handle, relayed_methods, event_tx, observe_blocked)
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

struct FakeComputer {
    handle: JoinHandle<()>,
    events: mpsc::UnboundedSender<(String, Value)>,
    server_messages: Arc<Mutex<Vec<Value>>>,
}

async fn connect_fake_computer(base_url: &str, token: &str, version: &str) -> JoinHandle<()> {
    connect_fake_computer_with_share_ack(base_url, token, version, false)
        .await
        .handle
}

async fn connect_fake_computer_with_share_ack(
    base_url: &str,
    token: &str,
    version: &str,
    advertise_share_ack: bool,
) -> FakeComputer {
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
    let mut capabilities = vec![
        "computer.status",
        "computer.observe",
        "computer.move",
        "computer.click",
        "computer.invoke",
        "computer.setValue",
        "computer.drag",
        "computer.scroll",
        "computer.typeText",
        "computer.key",
        "computer.share.start",
        "computer.share.status",
        "computer.share.stop",
        "computer.shell",
    ];
    if advertise_share_ack {
        capabilities.push("computer.share.ack");
    }
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
                "capabilities": capabilities
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let server_messages = Arc::new(Mutex::new(Vec::new()));
    let server_messages_log = server_messages.clone();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<(String, Value)>();
    let event_session_id = session_id.clone();
    let handle = tokio::spawn(async move {
        let (mut writer, mut reader) = socket.split();
        let mut frame_number = 0_u64;
        let mut current_frame = String::new();
        let mut event_sequence = 0_u64;
        let mut events_open = true;
        loop {
            let message = tokio::select! {
                event = event_rx.recv(), if events_open => {
                    let Some((name, data)) = event else {
                        events_open = false;
                        continue;
                    };
                    event_sequence += 1;
                    let envelope = json!({
                        "type": "event",
                        "name": name,
                        "data": data,
                        "protocolVersion": PROTOCOL_VERSION,
                        "sessionId": event_session_id,
                        "eventSequence": event_sequence,
                    });
                    writer
                        .send(Message::Text(envelope.to_string().into()))
                        .await
                        .unwrap();
                    continue;
                }
                message = reader.next() => message,
            };
            let Some(Ok(Message::Text(text))) = message else {
                break;
            };
            let message: Value = serde_json::from_str(text.as_str()).unwrap();
            if matches!(
                message.get("type").and_then(Value::as_str),
                Some("helloAck") | Some("eventAck")
            ) {
                server_messages_log.lock().unwrap().push(message.clone());
                continue;
            }
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
                                "imageWidth": 640,
                                "imageHeight": 400,
                                "screenX": 0,
                                "screenY": 0,
                                "screenWidth": 640,
                                "screenHeight": 400,
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
    });
    FakeComputer {
        handle,
        events: event_tx,
        server_messages,
    }
}

fn computer_share_frame_data(frame_id: &str, share: Value) -> Value {
    let pointer = json!({
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
        "updatedAt": "2026-08-19T00:00:00Z",
        "coordinateSpace": "image-pixels",
        "style": { "theme": "fixture", "fill": "#26C6FF", "outline": "#FFFFFF", "logicalSize": 42, "hotspot": "tip" }
    });
    json!({
        "screenshot": PIXEL,
        "frame": {
            "id": frame_id,
            "capturedAt": "2026-08-19T00:00:00Z",
            "windowId": "window-9",
            "pid": 4242,
            "appName": "Fixture",
            "windowTitle": "Shared target",
            "sessionMode": "background-window",
            "deliveryMode": "exact-window-background",
            "displayId": "window-9",
            "displayIndex": 0,
            "displayName": "Fixture — Shared target",
            "imageWidth": 640,
            "imageHeight": 400,
            "screenX": 0,
            "screenY": 0,
            "screenWidth": 640,
            "screenHeight": 400,
            "scaleFactor": 1.0,
            "rotation": 0.0,
            "pointer": pointer,
            "share": share
        }
    })
}

async fn wait_for_server_message(
    messages: &Arc<Mutex<Vec<Value>>>,
    predicate: impl Fn(&Value) -> bool,
    what: &str,
) -> Value {
    for _ in 0..100 {
        if let Some(found) = messages
            .lock()
            .unwrap()
            .iter()
            .find(|message| predicate(message))
            .cloned()
        {
            return found;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("{what} did not arrive");
}

async fn wait_for_computer_frame(
    client: &Client,
    base_url: &str,
    token: &str,
    frame_id: &str,
) -> Value {
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
        if state["state"]["computerObservation"]["frameId"] == frame_id {
            return state;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("share frame {frame_id} was not stored");
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
    let (fake, _relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
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
async fn paces_negotiated_share_frames_with_event_acks_and_drop_metrics() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let fake = connect_fake_computer_with_share_ack(&base_url, &token, VERSION, true).await;
    let client = Client::new();
    let state = wait_for_computer(&client, &base_url, &token).await;
    assert!(
        state["state"]["computer"]["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("computer.share.ack")),
        "the negotiated capability must survive the server-side allowlist"
    );
    let session_id = state["state"]["computer"]["sessionId"]
        .as_str()
        .unwrap()
        .to_owned();

    let hello_ack = wait_for_server_message(
        &fake.server_messages,
        |message| message["type"] == "helloAck",
        "helloAck",
    )
    .await;
    assert_eq!(hello_ack["ok"], true);
    assert_eq!(hello_ack["shareAck"], true);

    fake.events
        .send((
            "computer.share.frame".to_owned(),
            computer_share_frame_data(
                "frame-share-5",
                json!({
                    "active": true,
                    "id": "share-fixture",
                    "windowId": "window-9",
                    "fps": 4,
                    "sequence": 5,
                    "startedAt": "2026-08-19T00:00:00Z",
                    "captureScope": "exact-window",
                    "cursorComposited": true,
                    "droppedFrames": 3,
                    "ackPaced": true,
                    "lastAckedSequence": 4,
                    "backpressure": "latest-frame-wins"
                }),
            ),
        ))
        .unwrap();
    let state = wait_for_computer_frame(&client, &base_url, &token, "frame-share-5").await;
    let share = &state["state"]["computer"]["share"];
    assert_eq!(share["active"], true);
    assert_eq!(share["sequence"], 5);
    assert_eq!(share["ackPaced"], true);
    assert_eq!(share["droppedFrames"], 3);
    assert_eq!(share["lastAckedSequence"], 4);
    assert_eq!(share["backpressure"], "latest-frame-wins");
    let observation_share = &state["state"]["computerObservation"]["share"];
    assert_eq!(observation_share["ackPaced"], true);
    assert_eq!(observation_share["droppedFrames"], 3);
    assert_eq!(observation_share["lastAckedSequence"], 4);

    let ack = wait_for_server_message(
        &fake.server_messages,
        |message| message["type"] == "eventAck" && message["sequence"] == 5,
        "eventAck for share sequence 5",
    )
    .await;
    assert_eq!(ack["name"], "computer.share.frame");
    assert_eq!(ack["protocolVersion"], PROTOCOL_VERSION);
    assert_eq!(ack["sessionId"].as_str(), Some(session_id.as_str()));

    fake.events
        .send((
            "computer.share.frame".to_owned(),
            computer_share_frame_data(
                "frame-share-6",
                json!({
                    "active": true,
                    "id": "share-fixture",
                    "windowId": "window-9",
                    "fps": 4,
                    "sequence": 6,
                    "startedAt": "2026-08-19T00:00:00Z",
                    "captureScope": "exact-window",
                    "cursorComposited": true,
                    "droppedFrames": 3,
                    "ackPaced": true,
                    "lastAckedSequence": 5,
                    "backpressure": "latest-frame-wins"
                }),
            ),
        ))
        .unwrap();
    wait_for_computer_frame(&client, &base_url, &token, "frame-share-6").await;
    wait_for_server_message(
        &fake.server_messages,
        |message| message["type"] == "eventAck" && message["sequence"] == 6,
        "eventAck for share sequence 6",
    )
    .await;

    fake.handle.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn keeps_legacy_timer_shares_without_event_acks() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let fake = connect_fake_computer_with_share_ack(&base_url, &token, VERSION, false).await;
    let client = Client::new();
    let state = wait_for_computer(&client, &base_url, &token).await;
    assert!(
        !state["state"]["computer"]["capabilities"]
            .as_array()
            .unwrap()
            .contains(&json!("computer.share.ack"))
    );

    let hello_ack = wait_for_server_message(
        &fake.server_messages,
        |message| message["type"] == "helloAck",
        "helloAck",
    )
    .await;
    assert_eq!(hello_ack["ok"], true);
    assert_eq!(
        hello_ack["shareAck"], false,
        "a helper without the capability must not be promised acks"
    );

    fake.events
        .send((
            "computer.share.frame".to_owned(),
            computer_share_frame_data(
                "frame-legacy-1",
                json!({
                    "active": true,
                    "id": "share-legacy",
                    "windowId": "window-9",
                    "fps": 4,
                    "sequence": 1,
                    "startedAt": "2026-08-19T00:00:00Z",
                    "captureScope": "exact-window",
                    "cursorComposited": true
                }),
            ),
        ))
        .unwrap();
    let state = wait_for_computer_frame(&client, &base_url, &token, "frame-legacy-1").await;
    let share = &state["state"]["computer"]["share"];
    assert_eq!(share["active"], true);
    assert_eq!(share["ackPaced"], false);
    assert_eq!(share["droppedFrames"], 0);
    assert_eq!(share["lastAckedSequence"], 0);
    assert_eq!(share["backpressure"], "producer-blocking");
    assert_eq!(
        state["state"]["computerObservation"]["share"]["ackPaced"],
        false
    );

    // The stored frame proves the event was fully processed; give a stray ack
    // time to arrive before asserting that none was ever sent.
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        fake.server_messages
            .lock()
            .unwrap()
            .iter()
            .all(|message| message["type"] != "eventAck"),
        "legacy helpers must keep the exact timer behavior with no acks"
    );

    fake.handle.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn provisional_sockets_are_bounded_and_cannot_evict_a_ready_extension() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, _relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
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

async fn raw_response(port: u16, request: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut chunk))
            .await
            .expect("raw response deadline")
            .unwrap();
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    String::from_utf8_lossy(&buffer).into_owned()
}

#[tokio::test]
async fn rejects_non_loopback_host_headers_before_routing_and_auth() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let port: u16 = base_url.rsplit(':').next().unwrap().parse().unwrap();

    let health_rejected = raw_response(
        port,
        "GET /health HTTP/1.1\r\nHost: evil.example\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(
        health_rejected.starts_with("HTTP/1.1 403"),
        "unexpected response: {health_rejected}"
    );
    assert!(health_rejected.contains("HOST_REJECTED"));

    let body = r#"{"method":"tabs.list","params":{}}"#;
    let command_rejected = raw_response(
        port,
        &format!(
            "POST /api/v1/command HTTP/1.1\r\nHost: evil.example\r\nAuthorization: Bearer {token}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        ),
    )
    .await;
    assert!(
        command_rejected.starts_with("HTTP/1.1 403"),
        "unexpected response: {command_rejected}"
    );
    assert!(command_rejected.contains("HOST_REJECTED"));

    for good_host in [format!("127.0.0.1:{port}"), format!("localhost:{port}")] {
        let accepted = raw_response(
            port,
            &format!("GET /health HTTP/1.1\r\nHost: {good_host}\r\nConnection: close\r\n\r\n"),
        )
        .await;
        assert!(
            accepted.starts_with("HTTP/1.1 200"),
            "Host {good_host} was rejected: {accepted}"
        );
    }

    // A WebSocket upgrade with a rebound Host never reaches authentication:
    // the server answers 403 instead of 101 and sends no auth challenge.
    let upgrade_rejected = raw_response(
        port,
        "GET /bridge HTTP/1.1\r\nHost: evil.example\r\nOrigin: chrome-extension://aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\r\nConnection: Upgrade\r\nUpgrade: websocket\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Key: AAAAAAAAAAAAAAAAAAAAAA==\r\n\r\n",
    )
    .await;
    assert!(
        upgrade_rejected.starts_with("HTTP/1.1 403"),
        "unexpected response: {upgrade_rejected}"
    );

    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn attaches_recovery_taxonomy_to_failed_commands() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, _relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    let response = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "page.click", "params": { "ref": "e1", "generation": "g1" } }))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), 409);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "STALE_SNAPSHOT");
    assert_eq!(body["taxonomy"]["code"], "stale_snapshot");
    assert_eq!(body["taxonomy"]["retriable"], true);
    assert_eq!(body["taxonomy"]["recoveryHint"], "reobserve");
    assert!(!body["taxonomy"]["prose"].as_str().unwrap().is_empty());

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

/// The live 0.11.0 defect: a human clicked the in-page Stop button, and the
/// commands that came back `HUMAN_CONTROL_PAUSED` were reported to REST
/// clients as HTTP 500 "server error" — telling them to retry or alert when
/// the only thing that resolves it is a human resuming control.
#[tokio::test]
async fn a_human_paused_command_is_locked_rather_than_a_server_error() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, _relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    let response = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "browser.control.start", "params": { "tabId": 9 } }))
        .send()
        .await
        .unwrap();
    let status = response.status();
    assert_ne!(status, 500, "a human pause was reported as a server fault");
    assert_eq!(status, 423, "a held human pause is a lock, not a fault");
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["ok"], false);
    assert_eq!(body["error"]["code"], "HUMAN_CONTROL_PAUSED");
    assert_eq!(body["taxonomy"]["code"], "needs_user");
    assert_eq!(body["taxonomy"]["retriable"], false);
    assert_eq!(body["taxonomy"]["recoveryHint"], "handback");
    assert!(!body["taxonomy"]["prose"].as_str().unwrap().is_empty());

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn replays_completed_commands_with_the_same_call_id_without_redispatching() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    let request = json!({
        "method": "page.evaluate",
        "params": { "tabId": 7, "expression": "document.title" },
        "callId": "call-replay-1"
    });
    let first: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&request)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(first["ok"], true);
    assert_eq!(first["callId"], "call-replay-1");
    assert!(first.get("replayed").is_none());

    let second: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&request)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(second["ok"], true);
    assert_eq!(second["callId"], "call-replay-1");
    assert_eq!(second["replayed"], true);
    assert_eq!(
        serde_json::to_string(&second["result"]).unwrap(),
        serde_json::to_string(&first["result"]).unwrap()
    );

    let evaluate_relays = relayed_methods
        .lock()
        .unwrap()
        .iter()
        .filter(|method| method.as_str() == "page.evaluate")
        .count();
    assert_eq!(evaluate_relays, 1, "the duplicate must not re-dispatch");

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn refuses_a_concurrent_duplicate_call_id_with_a_conflict() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    let request = json!({
        "method": "page.typeText",
        "params": { "tabId": 7, "generation": "g1", "text": "hello" },
        "callId": "call-concurrent-1"
    });
    let first = {
        let client = client.clone();
        let base_url = base_url.clone();
        let token = token.clone();
        let request = request.clone();
        tokio::spawn(async move {
            client
                .post(format!("{base_url}/api/v1/command"))
                .bearer_auth(&token)
                .json(&request)
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()
        })
    };
    // Wait until the first command has actually been relayed, so it is
    // provably still in flight when the duplicate arrives.
    for _ in 0..200 {
        if relayed_methods
            .lock()
            .unwrap()
            .iter()
            .any(|method| method == "page.typeText")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let duplicate = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(duplicate.status(), 409);
    let duplicate: Value = duplicate.json().await.unwrap();
    assert_eq!(duplicate["error"]["code"], "CALL_IN_PROGRESS");
    assert_eq!(duplicate["callId"], "call-concurrent-1");

    let first = first.await.unwrap();
    assert_eq!(first["ok"], true);
    assert_eq!(first["result"]["typed"], true);
    assert_eq!(first["callId"], "call-concurrent-1");

    let type_relays = relayed_methods
        .lock()
        .unwrap()
        .iter()
        .filter(|method| method.as_str() == "page.typeText")
        .count();
    assert_eq!(type_relays, 1, "the concurrent duplicate must not dispatch");

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn caches_an_outcome_unknown_failure_when_the_client_disconnects_mid_command() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    // The fixture's page.typeText arm stays in flight for 250ms; a 50ms
    // client timeout drops the connection mid-command, after the envelope
    // was already dispatched to the extension.
    let request = json!({
        "method": "page.typeText",
        "params": { "tabId": 7, "generation": "g1", "text": "hello" },
        "callId": "call-dropped-1"
    });
    let interrupted = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .timeout(Duration::from_millis(50))
        .json(&request)
        .send()
        .await;
    assert!(
        interrupted.is_err(),
        "the client must disconnect before the command completes"
    );

    // A retry with the same callId must never re-dispatch: it returns the
    // cached synthetic outcome-unknown failure once the dropped handler has
    // recorded it (retries racing that record see CALL_IN_PROGRESS).
    let mut replayed = None;
    for _ in 0..200 {
        let retry: Value = client
            .post(format!("{base_url}/api/v1/command"))
            .bearer_auth(&token)
            .json(&request)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        if retry["error"]["code"] == "CALL_IN_PROGRESS" {
            tokio::time::sleep(Duration::from_millis(10)).await;
            continue;
        }
        replayed = Some(retry);
        break;
    }
    let replayed = replayed.expect("the interrupted callId must settle");
    assert_eq!(replayed["ok"], false);
    assert_eq!(replayed["error"]["code"], "COMMAND_OUTCOME_UNKNOWN");
    assert_eq!(replayed["taxonomy"]["code"], "outcome_unknown");
    assert_eq!(replayed["taxonomy"]["retriable"], false);
    assert_eq!(replayed["taxonomy"]["recoveryHint"], "reobserve");
    assert_eq!(replayed["callId"], "call-dropped-1");
    assert_eq!(replayed["replayed"], true);

    let type_relays = relayed_methods
        .lock()
        .unwrap()
        .iter()
        .filter(|method| method.as_str() == "page.typeText")
        .count();
    assert_eq!(
        type_relays, 1,
        "the interrupted command must reach the extension exactly once"
    );

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn refuses_a_call_id_reused_for_a_different_command() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    let first: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.evaluate",
            "params": { "tabId": 7, "expression": "document.title" },
            "callId": "call-reused-1"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(first["ok"], true);

    // The same callId with a different method is reuse, not a replay: the
    // cached page.evaluate response must never be returned for page.hover,
    // and nothing is dispatched.
    let relayed_before = relayed_methods.lock().unwrap().len();
    let reused = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.hover",
            "params": { "tabId": 7, "ref": "g1.e1", "generation": "g1" },
            "callId": "call-reused-1"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(reused.status(), 409);
    let reused: Value = reused.json().await.unwrap();
    assert_eq!(reused["error"]["code"], "CALL_ID_REUSED");
    assert_eq!(reused["taxonomy"]["code"], "invalid_request");
    assert_eq!(reused["taxonomy"]["retriable"], false);
    assert_eq!(reused["callId"], "call-reused-1");
    assert!(reused.get("replayed").is_none());
    assert_eq!(relayed_methods.lock().unwrap().len(), relayed_before);

    // Different params under the same callId are refused the same way.
    let changed_params = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.evaluate",
            "params": { "tabId": 7, "expression": "document.body.innerText" },
            "callId": "call-reused-1"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(changed_params.status(), 409);
    let changed_params: Value = changed_params.json().await.unwrap();
    assert_eq!(changed_params["error"]["code"], "CALL_ID_REUSED");
    assert_eq!(relayed_methods.lock().unwrap().len(), relayed_before);

    // The exact original command still replays from the cache.
    let replayed: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.evaluate",
            "params": { "tabId": 7, "expression": "document.title" },
            "callId": "call-reused-1"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(replayed["ok"], true);
    assert_eq!(replayed["replayed"], true);
    assert_eq!(relayed_methods.lock().unwrap().len(), relayed_before);

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn converts_normalized1000_computer_clicks_and_exposes_stable_content_hashes() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let fake = connect_fake_computer(&base_url, &token, VERSION).await;
    let client = Client::new();
    wait_for_computer(&client, &base_url, &token).await;

    // Without any observed frame, normalized coordinates cannot be grounded.
    let ungrounded = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "computer.click",
            "params": { "frameId": "frame-0", "x": 500, "y": 250, "coordinateSpace": "normalized1000" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ungrounded.status(), 409);
    let ungrounded: Value = ungrounded.json().await.unwrap();
    assert_eq!(ungrounded["error"]["code"], "NO_COMPUTER_FRAME");
    assert_eq!(ungrounded["taxonomy"]["code"], "stale_snapshot");
    assert_eq!(ungrounded["taxonomy"]["recoveryHint"], "reobserve");

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
    assert_eq!(observed["result"]["imageWidth"], 640);
    assert_eq!(observed["result"]["imageHeight"], 400);
    let first_hash = observed["state"]["computerObservation"]["contentHash"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(first_hash.len(), 64);
    assert_eq!(
        observed["state"]["computerObservation"]["screenshotWidth"],
        1
    );
    assert_eq!(
        observed["state"]["computerObservation"]["screenshotHeight"],
        1
    );

    // The helper receives image pixels: 500/1000 * 640 and 250/1000 * 400.
    let clicked: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "computer.click",
            "params": { "frameId": "frame-1", "x": 500, "y": 250, "coordinateSpace": "normalized1000" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(clicked["result"]["x"].as_f64().unwrap(), 320.0);
    assert_eq!(clicked["result"]["y"].as_f64().unwrap(), 100.0);

    // Identical fixture screenshots hash identically across observations.
    let second_hash = clicked["state"]["computerObservation"]["contentHash"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(second_hash, first_hash);

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn converts_normalized1000_page_clicks_against_the_observed_viewport() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, _relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    // Before any observation the viewport is unknown, so normalized
    // coordinates are refused with reobserve coaching.
    let ungrounded = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.clickAt",
            "params": { "tabId": 7, "generation": "g1", "x": 500, "y": 250, "coordinateSpace": "normalized1000" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(ungrounded.status(), 409);
    let ungrounded: Value = ungrounded.json().await.unwrap();
    assert_eq!(ungrounded["error"]["code"], "NO_BROWSER_OBSERVATION");
    assert_eq!(ungrounded["taxonomy"]["code"], "stale_snapshot");

    let observed: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "page.observe", "params": { "tabId": 7 } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let first_hash = observed["state"]["observation"]["contentHash"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(first_hash.len(), 64);
    assert_eq!(observed["state"]["observation"]["screenshotWidth"], 1);
    assert_eq!(observed["state"]["observation"]["screenshotHeight"], 1);

    // The extension receives viewport pixels: 500/1000 * 800 and 250/1000 * 600.
    let clicked: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.clickAt",
            "params": { "tabId": 7, "generation": "g1", "x": 500, "y": 250, "coordinateSpace": "normalized1000" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(clicked["result"]["receivedX"].as_f64().unwrap(), 400.0);
    assert_eq!(clicked["result"]["receivedY"].as_f64().unwrap(), 150.0);

    // Identical fixture screenshots hash identically across observations.
    let second_hash = clicked["state"]["observation"]["contentHash"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(second_hash, first_hash);

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn relays_wait_for_with_clamped_timeouts_and_no_control_bindings() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    // Activating the tab observes it, which stores an active browser control
    // session; a subsequent wait must still relay without control bindings.
    let activated: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "tabs.activate", "params": { "tabId": 7 } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(activated["state"]["browserControl"]["active"], true);

    let waited: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.waitFor",
            "params": { "tabId": 7, "text": "Welcome", "timeoutMs": 99_999 }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(waited["ok"], true);
    assert_eq!(waited["result"]["satisfied"], true);
    assert_eq!(waited["result"]["receivedTimeoutMs"], 12_000);
    assert_eq!(waited["result"]["receivedText"], "Welcome");
    assert!(waited["result"]["receivedControlSessionId"].is_null());
    assert!(
        relayed_methods
            .lock()
            .unwrap()
            .iter()
            .any(|method| method == "page.waitFor")
    );

    // At least one condition is required before anything is relayed.
    let unconditional = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "page.waitFor", "params": { "tabId": 7 } }))
        .send()
        .await
        .unwrap();
    assert_eq!(unconditional.status(), 400);

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn relays_hover_and_modifier_clicks_with_validated_refs() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    let clicked: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.click",
            "params": {
                "tabId": 7,
                "ref": "g1.e1",
                "generation": "g1",
                "button": "middle",
                "clickCount": 2,
                "modifiers": ["Shift", "Meta"]
            }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(clicked["ok"], true);
    assert_eq!(clicked["result"]["receivedRef"], "g1.e1");
    assert_eq!(clicked["result"]["receivedButton"], "middle");
    assert_eq!(clicked["result"]["receivedClickCount"], 2);
    assert_eq!(
        clicked["result"]["receivedModifiers"],
        json!(["Shift", "Meta"])
    );

    let hovered: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.hover",
            "params": { "tabId": 7, "ref": "g1.e2", "generation": "g1" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(hovered["ok"], true);
    assert_eq!(hovered["result"]["hovered"], true);
    assert_eq!(hovered["result"]["receivedRef"], "g1.e2");
    assert_eq!(hovered["result"]["receivedGeneration"], "g1");
    assert!(
        relayed_methods
            .lock()
            .unwrap()
            .iter()
            .any(|method| method == "page.hover")
    );

    // Malformed refs are refused by the server before any relay.
    let relayed_before = relayed_methods.lock().unwrap().len();
    let malformed = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.hover",
            "params": { "tabId": 7, "ref": "not a ref!", "generation": "g1" }
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(malformed.status(), 400);
    let malformed: Value = malformed.json().await.unwrap();
    assert_eq!(malformed["error"]["code"], "BAD_REQUEST");
    assert_eq!(malformed["taxonomy"]["code"], "invalid_request");
    assert_eq!(relayed_methods.lock().unwrap().len(), relayed_before);

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn relays_batches_with_per_step_sanitization_and_single_lease_bindings() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    // Observing first stores an active browser control session, so the batch
    // must receive the lease bindings exactly once at its top level.
    let observed: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "page.observe", "params": { "tabId": 7 } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(observed["state"]["browserControl"]["active"], true);

    let batched: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.batch",
            "params": {
                "tabId": 7,
                "generation": "g1",
                "actions": [
                    { "method": "page.fill", "ref": "g1.e1", "text": "hello" },
                    { "method": "page.key", "key": "ctrl+a" },
                    { "method": "page.click", "ref": "e2" }
                ]
            }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(batched["ok"], true);
    // The canned extension result reports a strict stop-at-first-failure run.
    assert_eq!(batched["result"]["completed"], 2);
    assert_eq!(batched["result"]["total"], 3);
    assert_eq!(batched["result"]["failedIndex"], 2);
    assert_eq!(batched["result"]["perStep"].as_array().unwrap().len(), 3);
    assert_eq!(batched["result"]["receivedGeneration"], "g1");
    // The lease binding is injected once at the top level only.
    assert_eq!(
        batched["result"]["receivedControlSessionId"],
        "control-fixture"
    );
    assert_eq!(batched["result"]["receivedTurn"], 3);
    assert_eq!(batched["result"]["receivedMoveSequence"], 5);
    let actions = batched["result"]["receivedActions"].as_array().unwrap();
    assert_eq!(actions.len(), 3);
    for (index, action) in actions.iter().enumerate() {
        assert_eq!(action["tabId"], 7, "actions[{index}] tab injection");
        assert_eq!(action["generation"], "g1", "actions[{index}] generation");
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
    assert_eq!(actions[2]["button"], "left");
    assert_eq!(actions[2]["modifiers"], json!([]));

    // Invalid batches are refused by the sanitizer before any relay.
    let relayed_before = relayed_methods.lock().unwrap().len();
    let eleven: Vec<Value> = (0..11)
        .map(|_| json!({ "method": "page.key", "key": "Enter" }))
        .collect();
    for params in [
        json!({ "tabId": 7, "generation": "g1", "actions": [{ "method": "page.batch", "actions": [] }] }),
        json!({ "tabId": 7, "generation": "g1", "actions": [{ "method": "page.evaluate", "expression": "1" }] }),
        json!({ "tabId": 7, "generation": "g1", "actions": [] }),
        json!({ "tabId": 7, "generation": "g1", "actions": eleven }),
    ] {
        let refused = client
            .post(format!("{base_url}/api/v1/command"))
            .bearer_auth(&token)
            .json(&json!({ "method": "page.batch", "params": params }))
            .send()
            .await
            .unwrap();
        assert_eq!(refused.status(), 400);
    }
    assert_eq!(relayed_methods.lock().unwrap().len(), relayed_before);

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

async fn wait_for_pending_dialog(
    client: &Client,
    base_url: &str,
    token: &str,
    expected_type: Option<&str>,
) -> Value {
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
        let dialog = &state["state"]["pendingDialog"];
        match expected_type {
            Some(expected) if dialog["type"] == expected => return state,
            None if dialog.is_null() => return state,
            _ => {}
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("pendingDialog never reached the expected state");
}

#[tokio::test]
async fn gates_mutations_behind_pending_dialogs_until_handled_or_closed() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, relayed_methods, events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    // The extension reports an opened dialog; /api/state publishes it.
    events
        .send((
            "page.dialogOpened".to_owned(),
            json!({ "tabId": 7, "type": "confirm", "message": "Proceed?", "hasPrompt": false, "at": 1 }),
        ))
        .unwrap();
    let state = wait_for_pending_dialog(&client, &base_url, &token, Some("confirm")).await;
    assert_eq!(state["state"]["pendingDialog"]["message"], "Proceed?");
    assert_eq!(state["state"]["pendingDialog"]["hasPrompt"], false);

    // Every renderer-touching command fails fast with zero envelopes
    // relayed: the dialog freezes the renderer main thread, so even the
    // read-only page.observe and page.waitFor would hang against it and
    // revoke the lease by timeout.
    let relayed_before = relayed_methods.lock().unwrap().len();
    for (method, params) in [
        (
            "page.click",
            json!({ "tabId": 7, "ref": "e1", "generation": "g1" }),
        ),
        ("tabs.activate", json!({ "tabId": 7 })),
        (
            "page.batch",
            json!({ "tabId": 7, "generation": "g1", "actions": [{ "method": "page.key", "key": "Enter" }] }),
        ),
        ("page.observe", json!({ "tabId": 7 })),
        ("page.waitFor", json!({ "tabId": 7, "text": "Welcome" })),
        ("browser.control.start", json!({ "tabId": 7 })),
    ] {
        let blocked = client
            .post(format!("{base_url}/api/v1/command"))
            .bearer_auth(&token)
            .json(&json!({ "method": method, "params": params }))
            .send()
            .await
            .unwrap();
        assert_eq!(blocked.status(), 409, "{method} must be gated");
        let blocked: Value = blocked.json().await.unwrap();
        assert_eq!(blocked["error"]["code"], "BLOCKED_BY_DIALOG");
        assert_eq!(blocked["taxonomy"]["code"], "blocked_by_dialog");
        assert_eq!(blocked["taxonomy"]["retriable"], true);
        assert_eq!(blocked["taxonomy"]["recoveryHint"], "resume");
    }
    assert_eq!(
        relayed_methods.lock().unwrap().len(),
        relayed_before,
        "gated commands must never be relayed"
    );

    // Browser-process reads stay allowed while the dialog is pending.
    let listed: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "tabs.list", "params": {} }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(listed["ok"], true);

    // page.handleDialog relays with sanitized params and lifts the gate.
    let handled: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.handleDialog",
            "params": { "tabId": 7, "accept": true, "promptText": "confirmed" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(handled["ok"], true);
    assert_eq!(handled["result"]["receivedAccept"], true);
    assert_eq!(handled["result"]["receivedPromptText"], "confirmed");
    assert!(handled["state"]["pendingDialog"].is_null());
    // The escape hatch is bound to the lease but never to an observation
    // turn: refreshing a turn needs an observation the dialog forbids.
    assert!(handled["result"]["receivedTurn"].is_null());
    assert!(handled["result"]["receivedMoveSequence"].is_null());

    // With the gate lifted, page.click relays again (the fixture answers it
    // with STALE_SNAPSHOT, proving the envelope reached the extension).
    let relayed: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "page.click", "params": { "tabId": 7, "ref": "e1", "generation": "g1" } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(relayed["error"]["code"], "STALE_SNAPSHOT");

    // A dialogClosed event also clears the gate without page.handleDialog.
    events
        .send((
            "page.dialogOpened".to_owned(),
            json!({ "tabId": 7, "type": "beforeunload", "message": "", "hasPrompt": false, "at": 2 }),
        ))
        .unwrap();
    wait_for_pending_dialog(&client, &base_url, &token, Some("beforeunload")).await;
    events
        .send((
            "page.dialogClosed".to_owned(),
            json!({ "tabId": 7, "accepted": false }),
        ))
        .unwrap();
    wait_for_pending_dialog(&client, &base_url, &token, None).await;
    let reopened: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "page.click", "params": { "tabId": 7, "ref": "e1", "generation": "g1" } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reopened["error"]["code"], "STALE_SNAPSHOT");

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn suppresses_the_auto_observe_when_a_dialog_opens_mid_action() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, relayed_methods, events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    // page.clickAt succeeds against the fixture and normally schedules a
    // delayed automatic page.observe; the dialog opened by the click must
    // suppress that observe, because it would hang against the frozen
    // renderer and revoke the lease.
    let pending = {
        let client = client.clone();
        let base_url = base_url.clone();
        let token = token.clone();
        tokio::spawn(async move {
            client
                .post(format!("{base_url}/api/v1/command"))
                .bearer_auth(&token)
                .json(&json!({
                    "method": "page.clickAt",
                    "params": { "tabId": 7, "generation": "g1", "x": 10, "y": 10 }
                }))
                .send()
                .await
                .unwrap()
                .json::<Value>()
                .await
                .unwrap()
        })
    };
    for _ in 0..200 {
        if relayed_methods
            .lock()
            .unwrap()
            .iter()
            .any(|method| method == "page.clickAt")
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // The dialog opens well inside the 350ms observation delay.
    events
        .send((
            "page.dialogOpened".to_owned(),
            json!({ "tabId": 7, "type": "confirm", "message": "Sure?", "hasPrompt": false, "at": 1 }),
        ))
        .unwrap();
    wait_for_pending_dialog(&client, &base_url, &token, Some("confirm")).await;

    let clicked = pending.await.unwrap();
    assert_eq!(clicked["ok"], true);
    let observe_relays = relayed_methods
        .lock()
        .unwrap()
        .iter()
        .filter(|method| method.as_str() == "page.observe")
        .count();
    assert_eq!(
        observe_relays, 0,
        "the pending dialog must suppress the automatic observation"
    );

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

/// The pre-observe gate cannot close the whole window: the dialog can open
/// between the gate and the extension receiving the relayed observation. The
/// live 0.10.0 run hit exactly that, and the extension then answered
/// `BLOCKED_BY_DIALOG`. That answer is a skipped observation, nothing more —
/// the published observation, the screenshot, and the lease all survive it.
#[tokio::test]
async fn an_auto_observe_refused_by_a_dialog_is_a_skip_not_a_lost_observation() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, relayed_methods, _events, observe_blocked) =
        connect_fake_extension_with_dialog_race(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    // A first, clean observation publishes state the race must not destroy.
    let observed: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "page.observe", "params": { "tabId": 7 } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(observed["ok"], true);
    assert_eq!(observed["state"]["observation"]["generation"], "g1");

    // The dialog opens after the server has already passed its own gate, so
    // the auto-observe is relayed and the extension refuses it.
    observe_blocked.store(true, Ordering::SeqCst);
    let relayed_before = relayed_methods
        .lock()
        .unwrap()
        .iter()
        .filter(|method| method.as_str() == "page.observe")
        .count();
    let clicked: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.clickAt",
            "params": { "tabId": 7, "generation": "g1", "x": 10, "y": 10 }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    // The action itself succeeded; the refused observation never turns it
    // into a failure.
    assert_eq!(clicked["ok"], true);
    assert_eq!(
        relayed_methods
            .lock()
            .unwrap()
            .iter()
            .filter(|method| method.as_str() == "page.observe")
            .count(),
        relayed_before + 1,
        "the auto-observe must be relayed to reproduce the race"
    );

    let state: Value = client
        .get(format!("{base_url}/api/state"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    // Nothing was cleared: the earlier observation, its screenshot binding,
    // and the browser-control lease are all exactly as they were.
    assert_eq!(state["state"]["observation"]["generation"], "g1");
    assert_eq!(state["state"]["observation"]["tabId"], 7);
    assert_eq!(state["state"]["browserControl"]["active"], true);
    assert_eq!(
        state["state"]["browserControl"]["sessionId"],
        "control-fixture"
    );

    // It is logged as a skip, not as an error.
    let skipped = state["state"]["activity"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| {
            entry["method"] == "page.observe"
                && entry["message"]
                    .as_str()
                    .is_some_and(|message| message.contains("skipped"))
        })
        .cloned()
        .expect("the refused auto-observe must be logged");
    assert_ne!(skipped["status"], "error");
    assert!(
        skipped["message"]
            .as_str()
            .unwrap()
            .contains("page.handleDialog")
    );

    // page.handleDialog stays usable in exactly this situation, and the very
    // next observation publishes again once the dialog is gone.
    let handled: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.handleDialog",
            "params": { "tabId": 7, "accept": false }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(handled["ok"], true);
    assert_eq!(handled["result"]["receivedAccept"], false);
    // The discarded observation left the extension a turn ahead of the
    // published state, so a turn-bound escape hatch would be unusable here.
    assert!(handled["result"]["receivedTurn"].is_null());
    assert_eq!(
        handled["result"]["receivedControlSessionId"],
        "control-fixture"
    );
    observe_blocked.store(false, Ordering::SeqCst);
    let reobserved: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "page.observe", "params": { "tabId": 7 } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(reobserved["ok"], true);
    assert_eq!(reobserved["state"]["observation"]["generation"], "g1");
    assert_eq!(reobserved["state"]["browserControl"]["active"], true);

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn frame_scoped_refs_relay_unchanged_and_malformed_ones_never_relay() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    // A well-formed frame ref reaches the extension byte for byte.
    let framed: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.click",
            "params": { "tabId": 7, "ref": "g1.f2.e5", "generation": "g1" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(framed["ok"], false);
    assert_eq!(framed["error"]["code"], "FRAME_DETACHED");
    assert!(
        framed["error"]["message"]
            .as_str()
            .unwrap()
            .contains("ref g1.f2.e5"),
        "the frame ref was not relayed verbatim: {}",
        framed["error"]["message"]
    );

    let relayed_before = relayed_methods.lock().unwrap().len();
    // Everything outside the published grammar is rejected before relay.
    for malformed in [
        "g1.f17.e5",
        "g1.f0.e5",
        "g1.f01.e5",
        "g1.f2.f3.e5",
        "g1.f2.e0",
        "G1.f2.e5",
    ] {
        let rejected = client
            .post(format!("{base_url}/api/v1/command"))
            .bearer_auth(&token)
            .json(&json!({
                "method": "page.click",
                "params": { "tabId": 7, "ref": malformed, "generation": "g1" }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(
            rejected.status(),
            reqwest::StatusCode::BAD_REQUEST,
            "{malformed} was not rejected"
        );
        let body: Value = rejected.json().await.unwrap();
        assert_eq!(body["taxonomy"]["code"], "invalid_request");
    }
    assert_eq!(
        relayed_methods.lock().unwrap().len(),
        relayed_before,
        "a malformed frame ref was relayed to the extension"
    );

    // page.hover accepts the same grammar; page.batch sub-steps do too.
    let hovered: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.hover",
            "params": { "tabId": 7, "ref": "mfz3k2ab-a1b2c3d4.f16.e9999", "generation": "mfz3k2ab-a1b2c3d4" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(hovered["ok"], true);
    assert_eq!(
        hovered["result"]["receivedRef"],
        "mfz3k2ab-a1b2c3d4.f16.e9999"
    );

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn observation_publishes_frame_provenance_and_summary() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, _relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    let observed: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "page.observe", "params": { "tabId": 8 } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(observed["ok"], true);
    let observation = &observed["state"]["observation"];
    assert_eq!(observation["generation"], "g2");

    // The element list stays bounded and reports that it was cut.
    let elements = observation["elements"].as_array().unwrap();
    assert_eq!(elements.len(), 250);
    assert_eq!(observation["elementsTruncated"], true);

    // The cap reserves a share for frame elements even though the extension
    // appends them after 300 top-document ones: an observation that
    // advertises a merged frame must publish refs the caller can act on.
    let published_frame_elements = elements
        .iter()
        .filter(|element| element["frameRef"] == "f1")
        .count();
    assert_eq!(published_frame_elements, 50);
    assert_eq!(elements.len() - published_frame_elements, 200);

    // Frame provenance rides along on the frame's own elements only.
    let framed = elements
        .iter()
        .find(|element| element["ref"] == "g2.f1.e1")
        .expect("the frame element was not published");
    assert_eq!(framed["frameRef"], "f1");
    assert_eq!(framed["frameId"], "6A1B");
    assert_eq!(framed["frameUrlOrigin"], "https://pay.example.test");
    assert_eq!(framed["crossOrigin"], true);
    assert_eq!(framed["bounds"]["x"], 125);
    assert_eq!(framed["bounds"]["y"], 69);
    let top = &elements[0];
    assert_eq!(top["ref"], "g2.e1");
    for absent in ["frameRef", "frameId", "frameUrlOrigin", "crossOrigin"] {
        assert!(
            top.get(absent).is_none(),
            "a top-document element published {absent}"
        );
    }

    // The frame list is sanitized: the malformed second entry is dropped.
    let frames = observation["frames"].as_array().unwrap();
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0]["ref"], "f1");
    assert_eq!(frames[0]["urlOrigin"], "https://pay.example.test");
    assert_eq!(frames[0]["crossOrigin"], true);
    assert_eq!(frames[0]["depth"], 1);
    assert_eq!(frames[0]["offset"], json!({ "x": 120, "y": 64 }));
    // A size is a size: no second origin next to `offset`.
    assert_eq!(frames[0]["size"], json!({ "width": 380, "height": 220 }));
    // The advertised count is what actually reached `elements`, not what the
    // extension merged before the publication cap.
    assert_eq!(frames[0]["elementCount"], 50);
    assert_eq!(frames[0]["truncated"], true);

    let summary = &observation["frameSummary"];
    assert_eq!(summary["supported"], true);
    assert_eq!(summary["mode"], "cdp-auto-attach");
    assert_eq!(summary["ownersSeen"], 3);
    assert_eq!(summary["attached"], 1);
    assert_eq!(summary["merged"], 1);
    let skipped = summary["skipped"].as_array().unwrap();
    assert_eq!(skipped.len(), 2);
    assert_eq!(skipped[1]["reason"], "same_process_frame");

    // /api/state republishes the same bounded observation.
    let state: Value = client
        .get(format!("{base_url}/api/state"))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let published = &state["state"]["observation"];
    assert_eq!(published["frames"][0]["ref"], "f1");
    assert_eq!(published["frameSummary"]["merged"], 1);
    assert_eq!(published["elementsTruncated"], true);
    let forged = published["elements"]
        .as_array()
        .unwrap()
        .iter()
        .find(|element| element["ref"] == "g2.e999");
    if let Some(forged) = forged {
        assert!(
            forged.get("crossOrigin").is_none(),
            "a top-document element forged cross-origin provenance"
        );
    }

    // A frameless observation is byte-identical to a pre-frame observation.
    let plain: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({ "method": "page.observe", "params": { "tabId": 7 } }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let plain_observation = &plain["state"]["observation"];
    for absent in ["frames", "frameSummary", "elementsTruncated"] {
        assert!(
            plain_observation.get(absent).is_none(),
            "a frameless observation published {absent}"
        );
    }
    assert!(
        plain_observation["elements"][0]
            .get("crossOrigin")
            .is_none(),
        "a frameless observation published cross-origin provenance"
    );

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

#[tokio::test]
async fn frame_failures_carry_the_frame_taxonomy() {
    let token = create_token();
    let (base_url, shutdown, handle) = start_server(&token).await;
    let (fake, _relayed_methods, _events) = connect_fake_extension(&base_url, &token).await;
    let client = Client::new();
    wait_for_tabs(&client, &base_url, &token).await;

    // A frame that changed under the action is a document change: retry after
    // a fresh observation.
    let detached: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.click",
            "params": { "tabId": 7, "ref": "g1.f1.e1", "generation": "g1" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(detached["error"]["code"], "FRAME_DETACHED");
    assert_eq!(detached["taxonomy"]["code"], "document_changed");
    assert_eq!(detached["taxonomy"]["retriable"], true);
    assert_eq!(detached["taxonomy"]["recoveryHint"], "reobserve");

    // A frame tree that moved is snapshot staleness.
    let stale: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.click",
            "params": { "tabId": 7, "ref": "g1.f1.e2", "generation": "g1" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(stale["error"]["code"], "STALE_FRAME_TREE");
    assert_eq!(stale["taxonomy"]["code"], "stale_snapshot");
    assert_eq!(stale["taxonomy"]["retriable"], true);

    // A refused capability is a request problem and must never be retried.
    let unsupported: Value = client
        .post(format!("{base_url}/api/v1/command"))
        .bearer_auth(&token)
        .json(&json!({
            "method": "page.click",
            "params": { "tabId": 7, "ref": "g1.f1.e3", "generation": "g1" }
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(unsupported["error"]["code"], "FRAME_ACTION_UNSUPPORTED");
    assert_eq!(unsupported["taxonomy"]["code"], "invalid_request");
    assert_eq!(unsupported["taxonomy"]["retriable"], false);

    fake.abort();
    let _ = shutdown.send(());
    handle.await.unwrap();
}

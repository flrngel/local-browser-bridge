use std::env;
use std::path::PathBuf;

use futures_util::{SinkExt as _, StreamExt as _};
use local_browser_bridge::{VERSION, load_or_create_token};
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
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".local-browser-bridge")
                .join("token")
        });
    let token = match env::var("LBB_TOKEN") {
        Ok(value) if !value.trim().is_empty() => value.trim().to_owned(),
        _ => load_or_create_token(&token_path).await?,
    };
    let url = format!("ws://127.0.0.1:{port}/bridge?token={token}");
    let mut request = url.into_client_request()?;
    request.headers_mut().insert(
        "Origin",
        "chrome-extension://local-browser-bridge-rust-mock".parse()?,
    );
    let (socket, _) = connect_async(request).await?;
    let (mut writer, mut reader) = socket.split();
    writer
        .send(Message::Text(
            json!({
                "type": "hello",
                "version": format!("{VERSION}-mock"),
                "browser": "Mock Chromium",
                "mode": "full-access",
                "capabilities": ["tabs.list", "page.observe", "page.click", "page.fill", "page.select", "page.clickAt", "page.typeText", "page.evaluate"]
            })
            .to_string()
            .into(),
        ))
        .await?;
    println!("Rust mock extension connected to ws://127.0.0.1:{port}/bridge");

    let mut display_name = String::new();
    let mut selected_color = "green".to_owned();
    while let Some(message) = reader.next().await {
        let message = message?;
        let Message::Text(text) = message else {
            continue;
        };
        let parsed: Value = serde_json::from_str(text.as_str())?;
        if parsed.get("type").and_then(Value::as_str) == Some("pong") {
            continue;
        }
        let Some(id) = parsed.get("id").and_then(Value::as_str) else {
            continue;
        };
        let method = parsed.get("method").and_then(Value::as_str).unwrap_or("");
        let params = parsed.get("params").cloned().unwrap_or_else(|| json!({}));
        let result = handle(method, &params, &mut display_name, &mut selected_color);
        writer
            .send(Message::Text(
                json!({ "id": id, "type": "result", "ok": true, "result": result })
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
        "status" => json!({ "connected": true, "mock": true, "fullAccess": true }),
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

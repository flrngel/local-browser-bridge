use std::env;
use std::path::PathBuf;
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use local_browser_bridge::computer::{
    COMPUTER_HELPER_ORIGIN, ComputerController, command_parts, result_envelope,
};
use local_browser_bridge::{VERSION, load_or_create_token};
use serde_json::{Value, json};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;

#[derive(Default)]
struct Cli {
    show_help: bool,
    show_version: bool,
    request_permissions: bool,
    benchmark: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = parse_args(env::args().skip(1))?;
    if cli.show_help {
        print_help();
        return Ok(());
    }
    if cli.show_version {
        println!("local-computer-helper {VERSION}");
        return Ok(());
    }
    let mut controller = ComputerController::new();
    if cli.request_permissions {
        println!(
            "{}",
            serde_json::to_string_pretty(&controller.request_permissions())?
        );
        return Ok(());
    }
    if cli.benchmark {
        println!(
            "{}",
            serde_json::to_string_pretty(&controller.benchmark(5)?)?
        );
        return Ok(());
    }

    let port = parse_port(env::var("LBB_PORT").ok().as_deref())?;
    let token_path = env::var_os("LBB_TOKEN_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_token_path);
    let token = match env::var("LBB_TOKEN").ok() {
        Some(token) if !token.trim().is_empty() => token.trim().to_owned(),
        _ => load_or_create_token(&token_path).await?,
    };

    println!("Local Computer Helper {VERSION}");
    println!("Non-interrupting background-window provider for Local Browser Bridge");
    println!("Connecting to 127.0.0.1:{port}; press Ctrl+C to stop.");
    println!("No global HID input or implicit foreground fallback is used.");
    println!(
        "No shell, filesystem, clipboard, process-launch, or telemetry capability is exposed."
    );

    let mut backoff = Duration::from_millis(250);
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("Stopping...");
                break;
            }
            result = run_session(port, &token, &mut controller) => {
                match result {
                    Ok(()) => {
                        backoff = Duration::from_millis(250);
                        eprintln!("Bridge connection closed; reconnecting.");
                    }
                    Err(error) => eprintln!("Bridge unavailable: {error}; reconnecting."),
                }
            }
        }
        tokio::select! {
            _ = tokio::signal::ctrl_c() => break,
            _ = tokio::time::sleep(backoff) => {}
        }
        backoff = (backoff * 2).min(Duration::from_secs(5));
    }
    Ok(())
}

async fn run_session(
    port: u16,
    token: &str,
    controller: &mut ComputerController,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut request =
        format!("ws://127.0.0.1:{port}/computer?token={token}").into_client_request()?;
    request
        .headers_mut()
        .insert("Origin", COMPUTER_HELPER_ORIGIN.parse()?);
    let (socket, _) = connect_async(request).await?;
    println!("Connected to Local Browser Bridge.");
    let (mut writer, mut reader) = socket.split();
    writer
        .send(Message::Text(controller.hello().to_string().into()))
        .await?;
    while let Some(message) = reader.next().await {
        match message? {
            Message::Text(text) => {
                let Ok(message) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                if message.get("type").and_then(Value::as_str) == Some("ping") {
                    writer
                        .send(Message::Text(json!({ "type": "pong" }).to_string().into()))
                        .await?;
                    continue;
                }
                let Some((id, method, params)) = command_parts(&message) else {
                    continue;
                };
                let id = id.to_owned();
                let method = method.to_owned();
                let response = result_envelope(&id, controller.execute(&method, &params));
                writer
                    .send(Message::Text(response.to_string().into()))
                    .await?;
            }
            Message::Ping(bytes) => writer.send(Message::Pong(bytes)).await?,
            Message::Close(_) => return Ok(()),
            _ => {}
        }
    }
    Ok(())
}

fn parse_args(arguments: impl Iterator<Item = String>) -> Result<Cli, String> {
    let mut cli = Cli::default();
    for argument in arguments {
        match argument.as_str() {
            "--help" | "-h" => cli.show_help = true,
            "--version" | "-V" => cli.show_version = true,
            "--request-permissions" => cli.request_permissions = true,
            "--benchmark" => cli.benchmark = true,
            _ => {
                return Err(format!(
                    "Unknown argument: {argument}. Use --help for usage."
                ));
            }
        }
    }
    Ok(cli)
}

fn print_help() {
    println!(
        "Local Computer Helper {VERSION}\n\n\
Usage: local-computer-helper [OPTIONS]\n\n\
Options:\n\
  --request-permissions   Request/check screen-capture and input permissions, then exit\n\
  --benchmark             Benchmark five screen observations, then exit\n\
  -V, --version           Print the installed version and exit\n\
  -h, --help              Print this help\n\n\
Without options, the helper connects to Local Browser Bridge on loopback."
    );
}

fn parse_port(raw: Option<&str>) -> Result<u16, String> {
    let raw = raw.unwrap_or("17373");
    let port = raw
        .parse::<u16>()
        .map_err(|_| "LBB_PORT must be an integer between 1 and 65535".to_owned())?;
    if port == 0 {
        return Err("LBB_PORT must be an integer between 1 and 65535".to_owned());
    }
    Ok(port)
}

fn default_token_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".local-browser-bridge")
        .join("token")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_helper_flags_and_ports() {
        let cli = parse_args(["--benchmark".to_owned()].into_iter()).unwrap();
        assert!(cli.benchmark);
        assert_eq!(parse_port(None).unwrap(), 17_373);
        assert!(parse_port(Some("0")).is_err());
        assert!(parse_args(["--unknown".to_owned()].into_iter()).is_err());
    }
}

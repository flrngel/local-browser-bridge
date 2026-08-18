use std::env;
use std::path::PathBuf;
use std::time::Duration;

use local_browser_bridge::{BridgeServer, ServerConfig, VERSION, load_or_create_token};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if env::args()
        .skip(1)
        .any(|argument| argument == "--version" || argument == "-V")
    {
        println!("local-browser-bridge {VERSION}");
        return Ok(());
    }
    let port = parse_port(env::var("LBB_PORT").ok().as_deref())?;
    let token_path = env::var_os("LBB_TOKEN_PATH")
        .map(PathBuf::from)
        .unwrap_or_else(default_token_path);
    let explicit_token = env::var("LBB_TOKEN").ok();
    let token = match explicit_token.as_deref() {
        Some(token) if !token.trim().is_empty() => token.trim().to_owned(),
        _ => load_or_create_token(&token_path).await?,
    };

    let mut config = ServerConfig::new(port, token.clone());
    config.call_timeout = Duration::from_secs(15);
    let server = BridgeServer::bind(config).await?;
    let address = server.local_addr()?;

    println!("Local Browser Bridge {VERSION}");
    println!("Control surface: http://127.0.0.1:{}", address.port());
    println!("Extension token: {token}");
    if explicit_token.is_some() {
        println!("Token source: LBB_TOKEN");
    } else {
        println!("Token file: {}", token_path.display());
    }
    println!("Standalone Rust server; Node.js is not required. Press Ctrl+C to stop.");

    server
        .serve(async {
            let _ = tokio::signal::ctrl_c().await;
            println!("Stopping...");
        })
        .await?;
    Ok(())
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
    fn validates_ports() {
        assert_eq!(parse_port(None).unwrap(), 17_373);
        assert_eq!(parse_port(Some("8080")).unwrap(), 8_080);
        assert!(parse_port(Some("0")).is_err());
        assert!(parse_port(Some("70000")).is_err());
    }
}

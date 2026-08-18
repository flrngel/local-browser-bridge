use std::env;
use std::path::PathBuf;
use std::time::Duration;

use local_browser_bridge::{
    BridgeServer, ServerConfig, UpdateState, VERSION, check_for_update, load_or_create_token,
};

#[derive(Default)]
struct Cli {
    show_help: bool,
    show_version: bool,
    check_updates: bool,
    no_update_check: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = parse_args(env::args().skip(1))?;
    if cli.show_help {
        print_help();
        return Ok(());
    }
    if cli.show_version {
        println!("local-browser-bridge {VERSION}");
        return Ok(());
    }
    if cli.check_updates {
        let update = check_for_update().await;
        println!("{}", update.message);
        if let Some(url) = update.release_url {
            println!("Release page: {url}");
        }
        if update.status == UpdateState::Error {
            return Err("Update check failed".into());
        }
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
    config.check_for_updates = !cli.no_update_check && !update_check_disabled_from_env()?;
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
    if cli.no_update_check || update_check_disabled_from_env()? {
        println!("Update check: disabled; no GitHub request was made.");
    } else {
        println!("Update check: GitHub release metadata only; no automatic download or install.");
    }

    server
        .serve(async {
            let _ = tokio::signal::ctrl_c().await;
            println!("Stopping...");
        })
        .await?;
    Ok(())
}

fn parse_args(arguments: impl Iterator<Item = String>) -> Result<Cli, String> {
    let mut cli = Cli::default();
    for argument in arguments {
        match argument.as_str() {
            "--help" | "-h" => cli.show_help = true,
            "--version" | "-V" => cli.show_version = true,
            "--check-updates" => cli.check_updates = true,
            "--no-update-check" => cli.no_update_check = true,
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
        "Local Browser Bridge {VERSION}\n\n\
Usage: local-browser-bridge [OPTIONS]\n\n\
Options:\n\
  --check-updates     Check official GitHub release metadata and exit\n\
  --no-update-check   Start without the one-time background metadata check\n\
  -V, --version       Print the installed version and exit\n\
  -h, --help          Print this help\n\n\
The update checker never downloads or installs files."
    );
}

fn update_check_disabled_from_env() -> Result<bool, String> {
    let Some(value) = env::var("LBB_DISABLE_UPDATE_CHECK").ok() else {
        return Ok(false);
    };
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" | "" => Ok(false),
        _ => Err("LBB_DISABLE_UPDATE_CHECK must be true/false or 1/0".to_owned()),
    }
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

    #[test]
    fn parses_update_flags_and_rejects_unknown_arguments() {
        let cli =
            parse_args(["--check-updates".to_owned(), "--no-update-check".to_owned()].into_iter())
                .unwrap();
        assert!(cli.check_updates);
        assert!(cli.no_update_check);
        assert!(parse_args(["--unknown".to_owned()].into_iter()).is_err());
    }
}

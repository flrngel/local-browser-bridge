use std::env;
use std::path::PathBuf;
use std::time::Duration;

use local_browser_bridge::{
    BridgeServer, ServerConfig, Settings, UpdateState, VERSION, check_for_update,
    default_settings_path, default_token_path, load_or_create_token, load_settings,
    print_license_report, resolve_shell_enabled,
};

#[derive(Default)]
struct Cli {
    show_help: bool,
    show_version: bool,
    show_licenses: bool,
    check_updates: bool,
    no_update_check: bool,
    enable_shell: bool,
    no_shell: bool,
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
    if cli.show_licenses {
        print_license_report("Local Browser Bridge")?;
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
    let explicit_token = env::var("LBB_TOKEN")
        .ok()
        .map(|token| token.trim().to_owned())
        .filter(|token| !token.is_empty());
    let (token, token_path) = match explicit_token {
        Some(token) => (token, None),
        None => {
            let path = match env::var_os("LBB_TOKEN_PATH") {
                Some(path) => PathBuf::from(path),
                None => default_token_path()?,
            };
            let token = load_or_create_token(&path).await?;
            (token, Some(path))
        }
    };

    let settings_path = default_settings_path().ok();
    let settings = match settings_path.as_ref() {
        Some(path) => load_settings(path).await,
        None => Settings::default(),
    };
    // Precedence (see `resolve_shell_enabled`): an explicit CLI flag wins;
    // otherwise LBB_ENABLE_SHELL decides in either direction when set;
    // otherwise the settings file decides (default on), so a fresh install
    // just works without a second setup step.
    let shell_enabled = resolve_shell_enabled(
        cli.enable_shell,
        cli.no_shell,
        env::var("LBB_ENABLE_SHELL").ok().as_deref(),
        settings.shell_enabled,
    )?;

    let mut config = ServerConfig::new(port, token.clone());
    config.call_timeout = Duration::from_secs(15);
    config.check_for_updates = !cli.no_update_check && !update_check_disabled_from_env()?;
    config.shell_enabled = shell_enabled;
    config.desktop_control_enabled = settings.desktop_control_enabled;
    config.settings_path = settings_path;
    let server = BridgeServer::bind(config).await?;
    let address = server.local_addr()?;
    let fetch_base_url = server.agent_fetch_base_url();

    println!("Local Browser Bridge {VERSION}");
    println!(
        "Control surface: http://127.0.0.1:{}/#token={token}",
        address.port()
    );
    println!("Extension token: {token}");
    println!("Agent Fetch base URL: {fetch_base_url}");
    match token_path {
        Some(path) => println!("Token file: {}", path.display()),
        None => println!("Token source: LBB_TOKEN"),
    }
    println!("Standalone Rust server; Node.js is not required. Press Ctrl+C to stop.");
    if shell_enabled {
        println!("Local shell: ENABLED (full current-user command access).");
    } else {
        println!(
            "Local shell: disabled; restart with --enable-shell, or enable it from the dashboard, to grant access."
        );
    }
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
            "--licenses" => cli.show_licenses = true,
            "--check-updates" => cli.check_updates = true,
            "--no-update-check" => cli.no_update_check = true,
            "--enable-shell" => cli.enable_shell = true,
            "--no-shell" => cli.no_shell = true,
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
  --enable-shell      Grant API clients full current-user native shell access\n\
  --no-shell          Force shell access off, overriding the settings file\n\
  --licenses          Print project and third-party license notices, then exit\n\
  -V, --version       Print the installed version and exit\n\
  -h, --help          Print this help\n\n\
Without --enable-shell or --no-shell, LBB_ENABLE_SHELL decides when set to\n\
1/true/yes/on or 0/false/no/off (empty/unset has no opinion); otherwise\n\
shell access follows settings.json (default: enabled). Passing both flags\n\
is an error. The update checker never downloads or installs files."
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
        assert!(!cli.enable_shell);
        assert!(
            parse_args(["--enable-shell".to_owned()].into_iter())
                .unwrap()
                .enable_shell
        );
        assert!(
            parse_args(["--licenses".to_owned()].into_iter())
                .unwrap()
                .show_licenses
        );
        assert!(parse_args(["--unknown".to_owned()].into_iter()).is_err());
    }

    #[test]
    fn parses_no_shell_flag() {
        let cli = parse_args(["--no-shell".to_owned()].into_iter()).unwrap();
        assert!(cli.no_shell);
        assert!(!cli.enable_shell);
    }

    /// Full precedence table, exercised the same way `main` calls
    /// `resolve_shell_enabled`: through a real parsed `Cli`.
    #[test]
    fn shell_precedence_matches_documented_rules() {
        let cli =
            |args: &[&str]| parse_args(args.iter().map(|argument| argument.to_string())).unwrap();

        // Flag beats env, in both directions.
        let enable = cli(&["--enable-shell"]);
        assert!(
            resolve_shell_enabled(enable.enable_shell, enable.no_shell, Some("0"), true).unwrap()
        );
        let no_shell = cli(&["--no-shell"]);
        assert!(
            !resolve_shell_enabled(no_shell.enable_shell, no_shell.no_shell, Some("1"), true)
                .unwrap()
        );

        // Both flags together is an error.
        let both = cli(&["--enable-shell", "--no-shell"]);
        assert!(resolve_shell_enabled(both.enable_shell, both.no_shell, None, true).is_err());

        // No flag: env beats settings, in both directions; empty env means
        // unset and settings decides; a bad env value is an error.
        let neither = cli(&[]);
        assert!(
            resolve_shell_enabled(neither.enable_shell, neither.no_shell, Some("on"), false)
                .unwrap()
        );
        assert!(
            !resolve_shell_enabled(neither.enable_shell, neither.no_shell, Some("off"), true)
                .unwrap()
        );
        assert!(
            resolve_shell_enabled(neither.enable_shell, neither.no_shell, Some(""), true).unwrap()
        );
        assert!(
            !resolve_shell_enabled(neither.enable_shell, neither.no_shell, None, false).unwrap()
        );
        assert!(
            resolve_shell_enabled(neither.enable_shell, neither.no_shell, Some("maybe"), true)
                .is_err()
        );
    }
}

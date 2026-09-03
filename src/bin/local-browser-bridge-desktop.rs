#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn main() {
    eprintln!("Local Browser Bridge Desktop is available only on macOS and Windows.");
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod desktop {
    use std::env;
    #[cfg(target_os = "macos")]
    use std::fs::File;
    use std::fs::{self, OpenOptions};
    use std::io::{self, Write as _};
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::thread;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    #[cfg(target_os = "windows")]
    use local_browser_bridge::setup;
    use local_browser_bridge::{
        BridgeServer, BridgeStatusMonitor, BridgeStatusSnapshot, ServerConfig, UpdateState,
        VERSION, default_settings_path, default_token_path, load_or_create_token, load_settings,
        print_license_report, write_embedded_extension,
    };
    use tao::event::{Event, StartCause};
    use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
    use tokio::sync::oneshot;
    use tray_icon::menu::{IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    #[cfg(test)]
    const DEFAULT_PORT: u16 = 17_373;
    /// Set (to any value) on the child process a successful first-run
    /// self-install relaunches into, so that process — not the installer
    /// process, which exits — knows to open the dashboard on its own. See
    /// `relaunch_installed` and `should_open_dashboard`.
    const JUST_INSTALLED_ENV: &str = "LBB_JUST_INSTALLED";
    const MENU_OPEN_DASHBOARD: &str = "open-dashboard";
    const MENU_EXTENSION_SETUP: &str = "extension-setup";
    const MENU_COPY_TOKEN: &str = "copy-token";
    const MENU_HELPER: &str = "computer-helper";
    const MENU_SHELL_TOGGLE: &str = "shell-status";
    const MENU_UPDATES: &str = "updates";
    const MENU_LOGS: &str = "logs";
    const MENU_QUIT: &str = "quit";
    #[cfg(target_os = "windows")]
    const MENU_UNINSTALL: &str = "uninstall";
    #[cfg(target_os = "windows")]
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    #[derive(Default)]
    struct Cli {
        show_help: bool,
        show_version: bool,
        show_licenses: bool,
        no_update_check: bool,
        enable_shell: bool,
        start_helper: bool,
        extension_setup: bool,
        #[cfg(target_os = "windows")]
        install: bool,
        #[cfg(target_os = "windows")]
        uninstall: bool,
        #[cfg(target_os = "windows")]
        no_install: bool,
    }

    // No `#[derive(Debug)]`: `ServerReady` carries a `BridgeStatusMonitor` and
    // a `tokio::runtime::Handle` (so the tray thread can react to live status
    // without its own runtime), and neither implements `Debug`. Nothing in
    // this file ever formats an `AppEvent` itself.
    //
    // `ServerReady` is intentionally larger than the other variants: it is a
    // one-shot event (sent exactly once per server start), not a hot-path
    // message, so boxing its fields would only add an allocation for no
    // measurable benefit.
    #[allow(clippy::large_enum_variant)]
    enum AppEvent {
        Menu(MenuEvent),
        ServerReady(
            BridgeStatusSnapshot,
            BridgeStatusMonitor,
            tokio::runtime::Handle,
        ),
        Status(BridgeStatusSnapshot),
        ServerFailed(String),
        ServerStopped,
    }

    pub fn main() {
        if let Err(error) = run() {
            write_desktop_log(&format!("Desktop Host failed: {error}"));
            platform::show_error("Local Browser Bridge could not start", &error.to_string());
            std::process::exit(1);
        }
    }

    fn run() -> Result<(), Box<dyn std::error::Error>> {
        let cli = parse_args(env::args().skip(1))?;
        if cli.show_help {
            println!("{}", help_text());
            return Ok(());
        }
        if cli.show_version {
            println!("local-browser-bridge-desktop {VERSION}");
            return Ok(());
        }
        if cli.show_licenses {
            print_license_report("Local Browser Bridge Desktop")?;
            return Ok(());
        }

        #[cfg(target_os = "windows")]
        if cli.install {
            return run_install_command();
        }
        #[cfg(target_os = "windows")]
        if cli.uninstall {
            return run_uninstall_command();
        }
        #[cfg(target_os = "windows")]
        if !cli.no_install
            && let Some(installed_path) = offer_first_run_install()
        {
            relaunch_installed(&installed_path)?;
            return Ok(());
        }
        if cli.start_helper {
            write_desktop_log(
                "--start-helper is no longer needed: the helper now starts automatically \
                 when desktop control is enabled",
            );
        }

        let port = parse_port(env::var("LBB_PORT").ok().as_deref())?;
        let bootstrap_runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let settings_path = default_settings_path()?;
        let settings = bootstrap_runtime.block_on(load_settings(&settings_path));
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
                let token = bootstrap_runtime.block_on(load_or_create_token(&path))?;
                (token, Some(path))
            }
        };
        drop(bootstrap_runtime);

        if cli.extension_setup {
            finish_extension_setup()?;
            return Ok(());
        }

        write_desktop_log(&format!("Desktop Host {VERSION} starting"));

        let lock_path = token_path
            .as_deref()
            .and_then(Path::parent)
            .unwrap_or_else(|| Path::new("."))
            .join("desktop.lock");
        let Some(instance) = SingleInstance::acquire(&lock_path)? else {
            open_dashboard(port, &token)?;
            return Ok(());
        };

        let event_loop = EventLoopBuilder::<AppEvent>::with_user_event().build();
        let proxy = event_loop.create_proxy();
        let menu_proxy = proxy.clone();
        MenuEvent::set_event_handler(Some(move |event| {
            let _ = menu_proxy.send_event(AppEvent::Menu(event));
        }));

        let mut config = ServerConfig::new(port, token.clone());
        config.call_timeout = Duration::from_secs(15);
        config.check_for_updates = !cli.no_update_check && !update_check_disabled_from_env()?;
        config.shell_enabled =
            cli.enable_shell || shell_enabled_from_env()? || settings.shell_enabled;
        config.desktop_control_enabled = settings.desktop_control_enabled;
        config.settings_path = Some(settings_path);
        config.extension_dir = install_root().ok().map(|root| root.join("extension"));
        let mut server = Some(ServerController::start(config, proxy)?);
        let mut ui: Option<DesktopUi> = None;
        let mut quit_requested = false;
        // Set only on the relaunched process of a just-completed first-run
        // self-install (see `relaunch_installed`); never on the installer
        // process itself, which already exited by this point.
        let just_installed = env::var_os(JUST_INSTALLED_ENV).is_some();
        let mut dashboard_opened = false;
        let _instance = instance;

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;
            match event {
                Event::NewEvents(StartCause::Init) => match DesktopUi::new(port, token.clone()) {
                    Ok(new_ui) => ui = Some(new_ui),
                    Err(error) => {
                        write_desktop_log(&format!("Could not create the tray icon: {error}"));
                        platform::show_error(
                            "Local Browser Bridge could not start",
                            &error.to_string(),
                        );
                        quit_requested = true;
                        *control_flow = ControlFlow::Exit;
                    }
                },
                Event::UserEvent(AppEvent::Menu(event)) => {
                    if let Some(ui) = ui.as_mut() {
                        match ui.handle_menu(event.id.as_ref()) {
                            MenuAction::Continue => {}
                            MenuAction::Quit => {
                                let _ = ui.stop_connected_helper();
                                quit_requested = true;
                                *control_flow = ControlFlow::Exit;
                            }
                        }
                    }
                }
                Event::UserEvent(AppEvent::ServerReady(status, monitor, runtime)) => {
                    if let Some(ui) = ui.as_mut() {
                        ui.set_server_handles(monitor, runtime);
                    }
                    if should_open_dashboard(
                        just_installed,
                        status.extension_connected,
                        dashboard_opened,
                    ) {
                        dashboard_opened = true;
                        if let Err(error) = open_dashboard(port, &token) {
                            // Never fails startup: a headless/CI run, or any
                            // environment with no browser to open, just logs
                            // this and carries on with the server and tray.
                            write_desktop_log(&format!(
                                "Could not open the dashboard automatically: {error}"
                            ));
                        }
                    }
                    if let Some(ui) = ui.as_mut() {
                        ui.apply_status(status);
                    }
                }
                Event::UserEvent(AppEvent::Status(status)) => {
                    if let Some(ui) = ui.as_mut() {
                        ui.apply_status(status);
                    }
                }
                Event::UserEvent(AppEvent::ServerFailed(error)) => {
                    write_desktop_log(&format!("Server failed: {error}"));
                    if let Some(ui) = ui.as_mut() {
                        ui.apply_error(&error);
                    }
                }
                Event::UserEvent(AppEvent::ServerStopped) => {
                    if !quit_requested && let Some(ui) = ui.as_mut() {
                        ui.apply_error("Server stopped unexpectedly");
                    }
                }
                Event::LoopDestroyed => {
                    if let Some(mut server) = server.take() {
                        server.shutdown();
                    }
                    write_desktop_log("Desktop Host stopped");
                }
                _ => {}
            }
        });
    }

    struct ServerController {
        shutdown: Option<oneshot::Sender<()>>,
        thread: Option<thread::JoinHandle<()>>,
    }

    impl ServerController {
        fn start(config: ServerConfig, proxy: EventLoopProxy<AppEvent>) -> Result<Self, io::Error> {
            let (shutdown_tx, shutdown_rx) = oneshot::channel();
            let thread = thread::Builder::new()
                .name("local-browser-bridge-server".to_owned())
                .spawn(move || {
                    let runtime = match tokio::runtime::Builder::new_multi_thread()
                        .enable_all()
                        .build()
                    {
                        Ok(runtime) => runtime,
                        Err(error) => {
                            let _ = proxy.send_event(AppEvent::ServerFailed(error.to_string()));
                            return;
                        }
                    };
                    runtime.block_on(async move {
                        let server = match BridgeServer::bind(config).await {
                            Ok(server) => server,
                            Err(error) => {
                                let _ = proxy.send_event(AppEvent::ServerFailed(error.to_string()));
                                return;
                            }
                        };
                        let monitor = server.status_monitor();
                        let first = monitor.snapshot().await;
                        let _ = proxy.send_event(AppEvent::ServerReady(
                            first.clone(),
                            monitor.clone(),
                            tokio::runtime::Handle::current(),
                        ));
                        let status_proxy = proxy.clone();
                        let status_task = tokio::spawn(async move {
                            let mut last = first;
                            let mut interval = tokio::time::interval(Duration::from_secs(1));
                            loop {
                                interval.tick().await;
                                let status = monitor.snapshot().await;
                                if status != last {
                                    if status_proxy
                                        .send_event(AppEvent::Status(status.clone()))
                                        .is_err()
                                    {
                                        break;
                                    }
                                    last = status;
                                }
                            }
                        });
                        let result = server
                            .serve(async move {
                                let _ = shutdown_rx.await;
                            })
                            .await;
                        status_task.abort();
                        match result {
                            Ok(()) => {
                                let _ = proxy.send_event(AppEvent::ServerStopped);
                            }
                            Err(error) => {
                                let _ = proxy.send_event(AppEvent::ServerFailed(error.to_string()));
                            }
                        }
                    });
                })?;
            Ok(Self {
                shutdown: Some(shutdown_tx),
                thread: Some(thread),
            })
        }

        fn shutdown(&mut self) {
            if let Some(shutdown) = self.shutdown.take() {
                let _ = shutdown.send(());
            }
            if let Some(thread) = self.thread.take() {
                let _ = thread.join();
            }
        }
    }

    impl Drop for ServerController {
        fn drop(&mut self) {
            self.shutdown();
        }
    }

    enum MenuAction {
        Continue,
        Quit,
    }

    struct DesktopUi {
        port: u16,
        token: String,
        tray: TrayIcon,
        server_status: MenuItem,
        browser_status: MenuItem,
        computer_status: MenuItem,
        shell_status: MenuItem,
        open_dashboard: MenuItem,
        helper: MenuItem,
        updates: MenuItem,
        status: Option<BridgeStatusSnapshot>,
        // Populated once, from `AppEvent::ServerReady`: lets menu clicks and
        // status-transition reactions call the sync `BridgeStatusMonitor`
        // setters and spawn background work directly from this (non-Tokio)
        // tray thread.
        monitor: Option<BridgeStatusMonitor>,
        runtime: Option<tokio::runtime::Handle>,
    }

    impl DesktopUi {
        fn new(port: u16, token: String) -> Result<Self, Box<dyn std::error::Error>> {
            let server_status =
                MenuItem::with_id("server-status", "Server: Starting…", false, None);
            let browser_status = MenuItem::with_id(
                "browser-status",
                "Browser: Waiting for extension",
                false,
                None,
            );
            let computer_status =
                MenuItem::with_id("computer-status", "Desktop control: Off", false, None);
            let shell_status =
                MenuItem::with_id(MENU_SHELL_TOGGLE, "Shell access: Off", true, None);
            let open_dashboard =
                MenuItem::with_id(MENU_OPEN_DASHBOARD, "Open Dashboard", false, None);
            let extension_setup = MenuItem::with_id(
                MENU_EXTENSION_SETUP,
                "Finish Browser Extension Setup",
                true,
                None,
            );
            let copy_token = MenuItem::with_id(MENU_COPY_TOKEN, "Copy Bridge Token", true, None);
            let helper = MenuItem::with_id(MENU_HELPER, "Start Computer Helper", false, None);
            let updates = MenuItem::with_id(MENU_UPDATES, "Check for Updates", true, None);
            let logs = MenuItem::with_id(MENU_LOGS, "Open Logs", true, None);
            let about = MenuItem::with_id(
                "about",
                format!("Local Browser Bridge {VERSION}"),
                false,
                None,
            );
            let quit = MenuItem::with_id(MENU_QUIT, "Quit Local Browser Bridge", true, None);
            let separator_one = PredefinedMenuItem::separator();
            let separator_two = PredefinedMenuItem::separator();
            let separator_three = PredefinedMenuItem::separator();
            #[cfg(target_os = "windows")]
            let uninstall_item =
                MenuItem::with_id(MENU_UNINSTALL, "Uninstall Local Browser Bridge", true, None);
            let mut items: Vec<&dyn IsMenuItem> = vec![
                &server_status,
                &browser_status,
                &computer_status,
                &shell_status,
                &separator_one,
                &open_dashboard,
                &extension_setup,
                &copy_token,
                &helper,
                &separator_two,
                &updates,
                &logs,
                &about,
                &separator_three,
            ];
            #[cfg(target_os = "windows")]
            items.push(&uninstall_item);
            items.push(&quit);
            let menu = Menu::with_items(&items)?;
            let tray = TrayIconBuilder::new()
                .with_id("local-browser-bridge")
                .with_menu(Box::new(menu))
                .with_tooltip("Local Browser Bridge — Starting")
                .with_icon(status_icon(IconState::Starting)?)
                .with_icon_as_template(cfg!(target_os = "macos"))
                .with_menu_on_left_click(true)
                .with_menu_on_right_click(true)
                .build()?;
            Ok(Self {
                port,
                token,
                tray,
                server_status,
                browser_status,
                computer_status,
                shell_status,
                open_dashboard,
                helper,
                updates,
                status: None,
                monitor: None,
                runtime: None,
            })
        }

        /// Called once, when the server first reports ready: gives the tray
        /// UI a way to act on live server state (toggle shell access, react
        /// to a desktop-control change, download/start the helper) without
        /// its own Tokio runtime.
        fn set_server_handles(
            &mut self,
            monitor: BridgeStatusMonitor,
            runtime: tokio::runtime::Handle,
        ) {
            self.monitor = Some(monitor);
            self.runtime = Some(runtime);
        }

        fn apply_status(&mut self, status: BridgeStatusSnapshot) {
            // Captured before `self.status` is overwritten below, so a
            // desktop-control toggle (from this tray or the web dashboard)
            // can be told apart from an unrelated status change and acted on
            // exactly once — never once per poll.
            let previous_desktop_control_enabled = self
                .status
                .as_ref()
                .map(|previous| previous.desktop_control_enabled);
            let desktop_control_enabled = status.desktop_control_enabled;
            let computer_connected = status.computer_connected;

            self.server_status.set_text("Server: Running");
            self.browser_status
                .set_text(if status.browser_control_active {
                    "Browser: Control active"
                } else if status.extension_connected {
                    "Browser: Connected"
                } else {
                    "Browser: Waiting for extension"
                });
            self.computer_status
                .set_text(if status.computer_share_active {
                    "Desktop control: Sharing active"
                } else if status.computer_connected {
                    "Desktop control: Connected"
                } else {
                    "Desktop control: Off"
                });
            self.shell_status.set_text(if status.shell_enabled {
                "Shell access: Enabled"
            } else {
                "Shell access: Off"
            });
            self.open_dashboard.set_enabled(true);
            self.helper.set_enabled(true);
            self.helper.set_text(if status.computer_connected {
                "Stop Computer Helper"
            } else {
                "Start Computer Helper"
            });
            match status.update_state {
                UpdateState::Available => self.updates.set_text(format!(
                    "Update Available: {}",
                    status.latest_version.as_deref().unwrap_or("new version")
                )),
                UpdateState::Checking => self.updates.set_text("Checking for Updates…"),
                _ => self.updates.set_text("Check for Updates"),
            }
            let icon_state = if status.browser_control_active || status.computer_share_active {
                IconState::Active
            } else if status.extension_connected {
                IconState::Connected
            } else {
                IconState::Waiting
            };
            let tooltip = if status.extension_connected {
                "Local Browser Bridge — Connected"
            } else {
                "Local Browser Bridge — Waiting for browser"
            };
            if let Ok(icon) = status_icon(icon_state) {
                let _ = self.tray.set_icon(Some(icon));
            }
            let _ = self.tray.set_tooltip(Some(tooltip));
            self.status = Some(status);

            let transitioned = previous_desktop_control_enabled != Some(desktop_control_enabled);
            if transitioned && desktop_control_enabled && !computer_connected {
                self.start_or_prepare_helper();
            } else if transitioned && !desktop_control_enabled && computer_connected {
                let _ = self.stop_connected_helper();
            }
        }

        fn apply_error(&mut self, error: &str) {
            self.server_status.set_text("Server: Error");
            self.browser_status.set_text("Browser: Unavailable");
            self.computer_status
                .set_text("Desktop control: Unavailable");
            self.open_dashboard.set_enabled(false);
            self.helper.set_enabled(false);
            if let Ok(icon) = status_icon(IconState::Error) {
                let _ = self.tray.set_icon(Some(icon));
            }
            let _ = self
                .tray
                .set_tooltip(Some(format!("Local Browser Bridge — {error}")));
        }

        fn handle_menu(&mut self, id: &str) -> MenuAction {
            #[cfg(target_os = "windows")]
            if id == MENU_UNINSTALL {
                self.perform_uninstall();
                return MenuAction::Quit;
            }
            let result = match id {
                MENU_OPEN_DASHBOARD => open_dashboard(self.port, &self.token),
                MENU_EXTENSION_SETUP => self.open_extension_setup(),
                MENU_COPY_TOKEN => platform::copy_text(&self.token),
                MENU_SHELL_TOGGLE => {
                    self.toggle_shell();
                    Ok(())
                }
                MENU_HELPER => {
                    if self
                        .status
                        .as_ref()
                        .is_some_and(|status| status.computer_connected)
                    {
                        self.stop_connected_helper()
                    } else {
                        self.start_helper()
                    }
                }
                MENU_UPDATES => platform::open_target(
                    "https://github.com/flrngel/local-browser-bridge/releases/latest",
                ),
                MENU_LOGS => platform::open_path(&logs_dir()),
                MENU_QUIT => return MenuAction::Quit,
                _ => Ok(()),
            };
            if let Err(error) = result {
                write_desktop_log(&format!("Menu action {id} failed: {error}"));
                platform::show_error("Action could not be completed", &error.to_string());
            }
            MenuAction::Continue
        }

        fn open_extension_setup(&self) -> io::Result<()> {
            finish_extension_setup()
        }

        fn start_helper(&mut self) -> io::Result<()> {
            let helper = helper_path()?;
            platform::start_helper(&helper)
        }

        fn stop_connected_helper(&mut self) -> io::Result<()> {
            let Some(status) = self
                .status
                .as_ref()
                .filter(|status| status.computer_connected)
            else {
                return Ok(());
            };
            let Some(process_id) = status.computer_controller_process_id else {
                return Ok(());
            };
            if process_id == std::process::id() {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "Refusing to terminate the Desktop Host process",
                ));
            }
            platform::terminate_process(process_id)
        }

        /// Flips shell access and asks the background settings write to
        /// persist it, so a tray toggle survives a restart exactly like the
        /// dashboard's does. Best-effort: with no server handles yet (the
        /// very first moment after launch) the click is simply ignored.
        fn toggle_shell(&mut self) {
            let (Some(monitor), Some(runtime)) = (self.monitor.as_ref(), self.runtime.as_ref())
            else {
                return;
            };
            let enabled = self
                .status
                .as_ref()
                .is_some_and(|status| status.shell_enabled);
            let new_value = !enabled;
            monitor.set_shell_enabled(new_value);
            runtime.spawn(async move {
                if let Ok(path) = default_settings_path() {
                    let mut settings = load_settings(&path).await;
                    settings.shell_enabled = new_value;
                    let _ = local_browser_bridge::save_settings(&path, &settings).await;
                }
            });
        }

        /// Starts the helper if it is already installed; if it is missing,
        /// hands off to the platform-specific recovery path (download it on
        /// Windows, report it as failed and unrecoverable on macOS) instead
        /// of failing silently.
        fn start_or_prepare_helper(&mut self) {
            match self.start_helper() {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    #[cfg(target_os = "windows")]
                    self.ensure_and_start_helper();
                    #[cfg(target_os = "macos")]
                    self.report_helper_missing();
                }
                Err(error) => {
                    write_desktop_log(&format!("Computer Helper could not start: {error}"));
                }
            }
        }

        #[cfg(target_os = "windows")]
        fn ensure_and_start_helper(&mut self) {
            let (Some(runtime), Some(monitor)) = (self.runtime.clone(), self.monitor.clone())
            else {
                return;
            };
            let Ok(root) = install_root() else {
                return;
            };
            runtime.spawn(async move {
                if let Some(path) = setup::ensure_helper_downloaded(&root, &monitor).await {
                    let _ = platform::start_helper(&path);
                }
            });
        }

        #[cfg(target_os = "macos")]
        fn report_helper_missing(&mut self) {
            let (Some(runtime), Some(monitor)) = (self.runtime.clone(), self.monitor.clone())
            else {
                return;
            };
            runtime.spawn(async move {
                monitor
                    .set_helper_setup(
                        "failed",
                        0,
                        "The Computer Helper was not found inside the app bundle. Reinstall Local Browser Bridge.",
                    )
                    .await;
            });
        }

        #[cfg(target_os = "windows")]
        fn perform_uninstall(&mut self) {
            let _ = self.stop_connected_helper();
            if let Err(error) = setup::uninstall(&mut |message| write_desktop_log(message)) {
                write_desktop_log(&format!("Uninstall could not finish: {error}"));
                platform::show_error(
                    "Local Browser Bridge could not be fully uninstalled",
                    &error.to_string(),
                );
            } else {
                write_desktop_log("Uninstalled Local Browser Bridge from the tray menu");
            }
        }
    }

    fn finish_extension_setup() -> io::Result<()> {
        let install_root = install_root()?;
        let extension = install_root.join("extension");
        if !extension.join("manifest.json").is_file() {
            write_embedded_extension(&extension)?;
        }
        platform::copy_text(extension.to_string_lossy().as_ref())?;
        platform::open_path(&extension)?;
        platform::open_extensions_page()
    }

    /// First-run flow: if this executable is not already the installed
    /// copy, asks the user (in plain language, naming exactly what
    /// happens) whether to set it up as an app. Returns the installed
    /// executable's path on an accepted, successful install; `None` in
    /// every other case (already installed, declined, or the install
    /// itself failed — the caller then just keeps running from here).
    #[cfg(target_os = "windows")]
    fn offer_first_run_install() -> Option<PathBuf> {
        if setup::is_installed().unwrap_or(false) {
            return None;
        }
        let accepted = platform::confirm(
            "Set up Local Browser Bridge?",
            "Local Browser Bridge can set itself up as an app:\n\n\
             \u{2022} Installs for your account only — no administrator needed\n\
             \u{2022} Starts automatically the next time you sign in\n\
             \u{2022} Adds a Start Menu shortcut and an uninstaller\n\n\
             Set it up now?",
        );
        if !accepted {
            write_desktop_log(
                "First-run install was declined; continuing from the current location",
            );
            return None;
        }
        let mut log_lines = Vec::new();
        match setup::install(&mut |message| log_lines.push(message.to_owned())) {
            Ok(path) => {
                for line in &log_lines {
                    write_desktop_log(line);
                }
                write_desktop_log(&format!(
                    "Installed Local Browser Bridge at {}",
                    path.display()
                ));
                Some(path)
            }
            Err(error) => {
                for line in &log_lines {
                    write_desktop_log(line);
                }
                write_desktop_log(&format!("Install failed: {error}"));
                platform::show_error(
                    "Local Browser Bridge could not set itself up",
                    &format!(
                        "{error}\n\nLocal Browser Bridge will keep running from its current location."
                    ),
                );
                None
            }
        }
    }

    /// `--install`: install unconditionally (no confirmation — the flag
    /// itself is the user's consent), then launch the installed copy.
    #[cfg(target_os = "windows")]
    fn run_install_command() -> Result<(), Box<dyn std::error::Error>> {
        let mut log_lines = Vec::new();
        match setup::install(&mut |message| log_lines.push(message.to_owned())) {
            Ok(path) => {
                for line in &log_lines {
                    write_desktop_log(line);
                }
                write_desktop_log(&format!(
                    "Installed Local Browser Bridge at {}",
                    path.display()
                ));
                relaunch_installed(&path)?;
                Ok(())
            }
            Err(error) => {
                for line in &log_lines {
                    write_desktop_log(line);
                }
                let message = format!("Local Browser Bridge could not be installed: {error}");
                write_desktop_log(&message);
                platform::show_error("Install failed", &message);
                Err(Box::new(error))
            }
        }
    }

    /// `--uninstall`: remove the install and exit, without starting the
    /// server or tray icon.
    #[cfg(target_os = "windows")]
    fn run_uninstall_command() -> Result<(), Box<dyn std::error::Error>> {
        match setup::uninstall(&mut |message| write_desktop_log(message)) {
            Ok(()) => {
                write_desktop_log("Uninstalled Local Browser Bridge via --uninstall");
                Ok(())
            }
            Err(error) => {
                let message =
                    format!("Local Browser Bridge could not be fully uninstalled: {error}");
                write_desktop_log(&message);
                platform::show_error("Uninstall failed", &message);
                Err(Box::new(error))
            }
        }
    }

    #[cfg(target_os = "windows")]
    fn relaunch_installed(path: &Path) -> io::Result<()> {
        use std::os::windows::process::CommandExt as _;
        let mut command = Command::new(path);
        command
            .env(JUST_INSTALLED_ENV, "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(CREATE_NO_WINDOW);
        command.spawn().map(|_| ())
    }

    #[derive(Clone, Copy)]
    enum IconState {
        Starting,
        Waiting,
        Connected,
        Active,
        Error,
    }

    fn status_icon(state: IconState) -> Result<Icon, tray_icon::BadIcon> {
        let color = if cfg!(target_os = "macos") {
            [0, 0, 0, 255]
        } else {
            match state {
                IconState::Starting => [100, 116, 139, 255],
                IconState::Waiting => [113, 113, 122, 255],
                IconState::Connected => [22, 163, 74, 255],
                IconState::Active => [37, 99, 235, 255],
                IconState::Error => [220, 38, 38, 255],
            }
        };
        let size = 32_u32;
        let mut rgba = vec![0_u8; (size * size * 4) as usize];
        for y in 0..size {
            for x in 0..size {
                let dx = x as i32 - 16;
                let dy = y as i32 - 16;
                let distance = dx * dx + dy * dy;
                let ring = (55..=156).contains(&distance);
                let bridge = (14..=18).contains(&(x as i32)) && (8..=24).contains(&(y as i32));
                if ring || bridge {
                    let offset = ((y * size + x) * 4) as usize;
                    rgba[offset..offset + 4].copy_from_slice(&color);
                }
            }
        }
        Icon::from_rgba(rgba, size, size)
    }

    fn install_root() -> io::Result<PathBuf> {
        if let Some(root) = env::var_os("LBB_INSTALL_ROOT") {
            return Ok(PathBuf::from(root));
        }
        let executable = env::current_exe()?;
        #[cfg(target_os = "windows")]
        {
            executable.parent().map(Path::to_path_buf).ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "Desktop executable has no parent")
            })
        }
        #[cfg(target_os = "macos")]
        {
            macos_install_root_from_executable(&executable).map_or_else(env::current_dir, Ok)
        }
    }

    #[cfg(target_os = "macos")]
    fn macos_install_root_from_executable(executable: &Path) -> Option<PathBuf> {
        let mut cursor = executable;
        while let Some(parent) = cursor.parent() {
            if cursor
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".app"))
            {
                return Some(parent.to_path_buf());
            }
            cursor = parent;
        }
        None
    }

    fn helper_path() -> io::Result<PathBuf> {
        let root = install_root()?;
        #[cfg(target_os = "macos")]
        {
            let helper = root.join("Local Computer Helper.app");
            if helper.is_dir() {
                return Ok(helper);
            }
        }
        #[cfg(target_os = "windows")]
        {
            for name in setup::helper_candidate_names(VERSION) {
                let candidate = root.join(name);
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Computer Helper was not found under {}", root.display()),
        ))
    }

    fn open_dashboard(port: u16, token: &str) -> io::Result<()> {
        platform::open_target(&format!("http://127.0.0.1:{port}/#token={token}"))
    }

    /// Decides whether this process should open the dashboard on its own,
    /// so a user is never left with nothing but a tray icon and no idea
    /// what to do next. Pure and platform-free (no I/O, no globals) so it
    /// is covered by a plain unit test on any host.
    ///
    /// - `just_installed`: this process is the relaunch of a first-run
    ///   self-install (see `JUST_INSTALLED_ENV`) — always worth a look.
    /// - `extension_connected`: the first status snapshot's read of whether
    ///   the browser extension has paired yet — while it hasn't, this user
    ///   still has setup to finish.
    /// - `already_opened`: this process already opened the dashboard once;
    ///   never again, so a login-start app never spawns a second tab.
    fn should_open_dashboard(
        just_installed: bool,
        extension_connected: bool,
        already_opened: bool,
    ) -> bool {
        if already_opened {
            return false;
        }
        just_installed || !extension_connected
    }

    fn logs_dir() -> PathBuf {
        #[cfg(target_os = "macos")]
        {
            env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Library/Logs/Local Browser Bridge")
        }
        #[cfg(target_os = "windows")]
        {
            env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join("Local Browser Bridge/logs")
        }
    }

    fn write_desktop_log(message: &str) {
        let directory = logs_dir();
        if fs::create_dir_all(&directory).is_err() {
            return;
        }
        let path = directory.join("desktop.log");
        if fs::metadata(&path).is_ok_and(|metadata| metadata.len() > 1_048_576) {
            let _ = fs::rename(&path, directory.join("desktop.log.1"));
        }
        let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
            return;
        };
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0);
        let bounded = message.chars().take(1_000).collect::<String>();
        let _ = writeln!(file, "{timestamp} {bounded}");
    }

    fn parse_args(arguments: impl Iterator<Item = String>) -> Result<Cli, String> {
        let mut cli = Cli::default();
        for argument in arguments {
            match argument.as_str() {
                "--help" | "-h" => cli.show_help = true,
                "--version" | "-V" => cli.show_version = true,
                "--licenses" => cli.show_licenses = true,
                "--no-update-check" => cli.no_update_check = true,
                "--enable-shell" => cli.enable_shell = true,
                "--start-helper" => cli.start_helper = true,
                "--extension-setup" => cli.extension_setup = true,
                #[cfg(target_os = "windows")]
                "--install" => cli.install = true,
                #[cfg(target_os = "windows")]
                "--uninstall" => cli.uninstall = true,
                #[cfg(target_os = "windows")]
                "--no-install" => cli.no_install = true,
                _ => {
                    return Err(format!(
                        "Unknown argument: {argument}. Use --help for usage."
                    ));
                }
            }
        }
        Ok(cli)
    }

    fn help_text() -> String {
        #[cfg(target_os = "windows")]
        let windows_only = "\
  --install            Install as an app under this account and exit\n\
  --uninstall          Remove the app installed under this account and exit\n\
  --no-install         Skip the one-time \"set up as an app?\" prompt\n\
";
        #[cfg(not(target_os = "windows"))]
        let windows_only = "";
        format!(
            "Local Browser Bridge Desktop {VERSION}\n\n\
Usage: local-browser-bridge-desktop [OPTIONS]\n\n\
Options:\n\
  --start-helper       Start the optional Computer Helper after the server\n\
  --extension-setup    Open the browser extension setup guide and exit\n\
  --no-update-check    Start without the background release metadata check\n\
  --enable-shell       Grant API clients full current-user native shell access\n\
{windows_only}\
  --licenses           Print project and third-party license notices, then exit\n\
  -V, --version        Print the installed version and exit\n\
  -h, --help           Print this help"
        )
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

    fn update_check_disabled_from_env() -> Result<bool, String> {
        parse_bool_env("LBB_DISABLE_UPDATE_CHECK")
    }

    fn shell_enabled_from_env() -> Result<bool, String> {
        parse_bool_env("LBB_ENABLE_SHELL")
    }

    fn parse_bool_env(name: &str) -> Result<bool, String> {
        let Some(value) = env::var(name).ok() else {
            return Ok(false);
        };
        match value.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" | "" => Ok(false),
            _ => Err(format!("{name} must be true/false or 1/0")),
        }
    }

    #[cfg(target_os = "macos")]
    struct SingleInstance {
        _file: File,
    }

    #[cfg(target_os = "macos")]
    impl SingleInstance {
        fn acquire(path: &Path) -> io::Result<Option<Self>> {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            let file = OpenOptions::new()
                .create(true)
                .read(true)
                .write(true)
                .truncate(false)
                .open(path)?;
            use std::os::fd::AsRawFd as _;
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if result == 0 {
                Ok(Some(Self { _file: file }))
            } else if io::Error::last_os_error().kind() == io::ErrorKind::WouldBlock {
                Ok(None)
            } else {
                Err(io::Error::last_os_error())
            }
        }
    }

    #[cfg(target_os = "windows")]
    struct SingleInstance {
        handle: *mut std::ffi::c_void,
    }

    #[cfg(target_os = "windows")]
    impl SingleInstance {
        fn acquire(_path: &Path) -> io::Result<Option<Self>> {
            let name = wide("Local\\LocalBrowserBridgeDesktop-v1");
            let handle = unsafe { platform::CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            if unsafe { platform::GetLastError() } == 183 {
                let _ = unsafe { platform::CloseHandle(handle) };
                Ok(None)
            } else {
                Ok(Some(Self { handle }))
            }
        }
    }

    #[cfg(target_os = "windows")]
    impl Drop for SingleInstance {
        fn drop(&mut self) {
            let _ = unsafe { platform::CloseHandle(self.handle) };
        }
    }

    #[cfg(target_os = "windows")]
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    mod platform {
        use super::*;

        #[cfg(target_os = "macos")]
        pub fn open_target(target: &str) -> io::Result<()> {
            command_status(Command::new("/usr/bin/open").arg(target))
        }

        #[cfg(target_os = "windows")]
        pub fn open_target(target: &str) -> io::Result<()> {
            shell_execute(target)
        }

        pub fn open_path(path: &Path) -> io::Result<()> {
            open_target(path.to_string_lossy().as_ref())
        }

        #[cfg(target_os = "macos")]
        pub fn open_extensions_page() -> io::Result<()> {
            let chrome = Path::new("/Applications/Google Chrome.app");
            let edge = Path::new("/Applications/Microsoft Edge.app");
            if chrome.is_dir() {
                command_status(Command::new("/usr/bin/open").args([
                    "-a",
                    "Google Chrome",
                    "chrome://extensions",
                ]))
            } else if edge.is_dir() {
                command_status(Command::new("/usr/bin/open").args([
                    "-a",
                    "Microsoft Edge",
                    "edge://extensions",
                ]))
            } else {
                Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    "Google Chrome or Microsoft Edge is required",
                ))
            }
        }

        #[cfg(target_os = "windows")]
        pub fn open_extensions_page() -> io::Result<()> {
            let mut candidates = Vec::new();
            for (variable, relative, target) in [
                (
                    "ProgramFiles",
                    "Google/Chrome/Application/chrome.exe",
                    "chrome://extensions",
                ),
                (
                    "ProgramFiles(x86)",
                    "Google/Chrome/Application/chrome.exe",
                    "chrome://extensions",
                ),
                (
                    "LOCALAPPDATA",
                    "Google/Chrome/Application/chrome.exe",
                    "chrome://extensions",
                ),
                (
                    "ProgramFiles(x86)",
                    "Microsoft/Edge/Application/msedge.exe",
                    "edge://extensions",
                ),
                (
                    "ProgramFiles",
                    "Microsoft/Edge/Application/msedge.exe",
                    "edge://extensions",
                ),
            ] {
                if let Some(root) = env::var_os(variable) {
                    let executable = PathBuf::from(root).join(relative);
                    if executable.is_file() {
                        candidates.push((executable, target));
                    }
                }
            }
            candidates.extend([
                (PathBuf::from("chrome.exe"), "chrome://extensions"),
                (PathBuf::from("msedge.exe"), "edge://extensions"),
            ]);
            for (executable, target) in candidates {
                let mut browser = Command::new(executable);
                browser.arg(target);
                no_window(&mut browser);
                if browser.spawn().is_ok() {
                    return Ok(());
                }
            }
            Err(io::Error::new(
                io::ErrorKind::NotFound,
                "Google Chrome or Microsoft Edge is required",
            ))
        }

        #[cfg(target_os = "macos")]
        pub fn copy_text(text: &str) -> io::Result<()> {
            let mut child = Command::new("/usr/bin/pbcopy")
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
            child
                .stdin
                .as_mut()
                .ok_or_else(|| io::Error::other("pbcopy stdin was unavailable"))?
                .write_all(text.as_bytes())?;
            let status = child.wait()?;
            if status.success() {
                Ok(())
            } else {
                Err(io::Error::other("pbcopy failed"))
            }
        }

        #[cfg(target_os = "windows")]
        pub fn copy_text(text: &str) -> io::Result<()> {
            let mut command = Command::new("clip.exe");
            command
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            no_window(&mut command);
            let mut child = command.spawn()?;
            child
                .stdin
                .as_mut()
                .ok_or_else(|| io::Error::other("clip.exe stdin was unavailable"))?
                .write_all(text.as_bytes())?;
            let status = child.wait()?;
            if status.success() {
                Ok(())
            } else {
                Err(io::Error::other("clip.exe failed"))
            }
        }

        #[cfg(target_os = "macos")]
        pub fn start_helper(path: &Path) -> io::Result<()> {
            command_status(Command::new("/usr/bin/open").arg(path))
        }

        #[cfg(target_os = "windows")]
        pub fn start_helper(path: &Path) -> io::Result<()> {
            let mut command = Command::new(path);
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            no_window(&mut command);
            command.spawn().map(|_| ())
        }

        #[cfg(target_os = "macos")]
        pub fn terminate_process(process_id: u32) -> io::Result<()> {
            let result = unsafe { libc::kill(process_id as i32, libc::SIGTERM) };
            if result == 0 {
                Ok(())
            } else {
                Err(io::Error::last_os_error())
            }
        }

        #[cfg(target_os = "windows")]
        pub fn terminate_process(process_id: u32) -> io::Result<()> {
            let handle = unsafe { OpenProcess(0x0001, 0, process_id) };
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let terminated = unsafe { TerminateProcess(handle, 0) };
            let error = (terminated == 0).then(io::Error::last_os_error);
            let _ = unsafe { CloseHandle(handle) };
            error.map_or(Ok(()), Err)
        }

        #[cfg(target_os = "macos")]
        pub fn show_error(title: &str, message: &str) {
            let script = format!(
                "display dialog {} with title {} buttons {{\"OK\"}} default button \"OK\" with icon caution",
                apple_script_string(message),
                apple_script_string(title)
            );
            let _ = Command::new("/usr/bin/osascript")
                .args(["-e", &script])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }

        #[cfg(target_os = "windows")]
        pub fn show_error(title: &str, message: &str) {
            let title = wide(title);
            let message = wide(message);
            let _ = unsafe {
                MessageBoxW(
                    std::ptr::null_mut(),
                    message.as_ptr(),
                    title.as_ptr(),
                    0x0000_0010,
                )
            };
        }

        /// Shows a Yes/No question dialog and returns whether the user
        /// chose Yes. Used only for the one-time first-run install prompt.
        #[cfg(target_os = "windows")]
        pub fn confirm(title: &str, message: &str) -> bool {
            const MB_YESNO: u32 = 0x0000_0004;
            const MB_ICONQUESTION: u32 = 0x0000_0020;
            const IDYES: i32 = 6;
            let title = wide(title);
            let message = wide(message);
            let result = unsafe {
                MessageBoxW(
                    std::ptr::null_mut(),
                    message.as_ptr(),
                    title.as_ptr(),
                    MB_YESNO | MB_ICONQUESTION,
                )
            };
            result == IDYES
        }

        #[cfg(target_os = "macos")]
        fn apple_script_string(value: &str) -> String {
            format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
        }

        #[cfg(target_os = "macos")]
        fn command_status(command: &mut Command) -> io::Result<()> {
            command.stdout(Stdio::null()).stderr(Stdio::null());
            let status = command.status()?;
            if status.success() {
                Ok(())
            } else {
                Err(io::Error::other("Platform launcher failed"))
            }
        }

        #[cfg(target_os = "windows")]
        fn no_window(command: &mut Command) {
            use std::os::windows::process::CommandExt as _;
            command.creation_flags(CREATE_NO_WINDOW);
        }

        #[cfg(target_os = "windows")]
        fn shell_execute(target: &str) -> io::Result<()> {
            let verb = wide("open");
            let target = wide(target);
            let result = unsafe {
                ShellExecuteW(
                    std::ptr::null_mut(),
                    verb.as_ptr(),
                    target.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    1,
                )
            } as isize;
            if result > 32 {
                Ok(())
            } else {
                Err(io::Error::other(format!(
                    "Windows could not open the requested target ({result})"
                )))
            }
        }

        #[cfg(target_os = "windows")]
        #[link(name = "kernel32")]
        unsafe extern "system" {
            pub fn CreateMutexW(
                attributes: *const std::ffi::c_void,
                initial_owner: i32,
                name: *const u16,
            ) -> *mut std::ffi::c_void;
            pub fn GetLastError() -> u32;
            pub fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
            fn OpenProcess(
                desired_access: u32,
                inherit_handle: i32,
                process_id: u32,
            ) -> *mut std::ffi::c_void;
            fn TerminateProcess(handle: *mut std::ffi::c_void, exit_code: u32) -> i32;
        }

        #[cfg(target_os = "windows")]
        #[link(name = "shell32")]
        unsafe extern "system" {
            fn ShellExecuteW(
                window: *mut std::ffi::c_void,
                operation: *const u16,
                file: *const u16,
                parameters: *const u16,
                directory: *const u16,
                show_command: i32,
            ) -> *mut std::ffi::c_void;
        }

        #[cfg(target_os = "windows")]
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBoxW(
                window: *mut std::ffi::c_void,
                text: *const u16,
                caption: *const u16,
                kind: u32,
            ) -> i32;
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn desktop_cli_keeps_shell_and_helper_opt_in() {
            let cli = parse_args([].into_iter()).unwrap();
            assert!(!cli.enable_shell);
            assert!(!cli.start_helper);
            let cli =
                parse_args(["--enable-shell".to_owned(), "--start-helper".to_owned()].into_iter())
                    .unwrap();
            assert!(cli.enable_shell);
            assert!(cli.start_helper);
        }

        #[test]
        fn desktop_port_validation_matches_the_server() {
            assert_eq!(parse_port(None).unwrap(), DEFAULT_PORT);
            assert_eq!(parse_port(Some("8080")).unwrap(), 8080);
            assert!(parse_port(Some("0")).is_err());
        }

        #[test]
        fn should_open_dashboard_covers_first_run_and_setup_incomplete_cases() {
            // Just installed: open regardless of connection state.
            assert!(should_open_dashboard(true, true, false));
            assert!(should_open_dashboard(true, false, false));
            // Extension not connected yet: this user still has setup to finish.
            assert!(should_open_dashboard(false, false, false));
            // Already opened once this run: never a second tab.
            assert!(!should_open_dashboard(true, true, true));
            assert!(!should_open_dashboard(true, false, true));
            assert!(!should_open_dashboard(false, false, true));
            // Connected and not a first run: stay quiet on a login-start app.
            assert!(!should_open_dashboard(false, true, false));
        }

        #[test]
        fn generated_icons_have_valid_dimensions() {
            for state in [
                IconState::Starting,
                IconState::Waiting,
                IconState::Connected,
                IconState::Active,
                IconState::Error,
            ] {
                assert!(status_icon(state).is_ok());
            }
        }

        #[cfg(target_os = "macos")]
        #[test]
        fn packaged_app_resolves_its_sibling_install_root() {
            let executable = Path::new(
                "/Users/example/Applications/Local Browser Bridge/Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop",
            );
            assert_eq!(
                macos_install_root_from_executable(executable),
                Some(PathBuf::from(
                    "/Users/example/Applications/Local Browser Bridge"
                ))
            );
        }
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn main() {
    desktop::main();
}

use std::collections::{HashMap, VecDeque};
use std::env;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt as _, StreamExt as _};
use local_browser_bridge::computer::{
    COMPUTER_HELPER_ORIGIN, CommandCancellation, ComputerController, ComputerError,
    NATIVE_COMPUTER_SUPPORTED, command_parts, result_envelope,
};
use local_browser_bridge::ws_auth::{
    AUTH_TIMEOUT, COMPUTER_CONNECTOR, ClientHello, MAX_AUTH_MESSAGE_BYTES, MAX_AUTH_MESSAGES,
};
use local_browser_bridge::{PROTOCOL_VERSION, VERSION, load_or_create_token};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::{WebSocketStream, connect_async};

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
    if !NATIVE_COMPUTER_SUPPORTED {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Native computer control is available only on macOS and Windows",
        )
        .into());
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

    let controller = Arc::new(Mutex::new(controller));
    let mut backoff = Duration::from_millis(250);
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("Stopping...");
                break;
            }
            result = run_session(port, &token, Arc::clone(&controller)) => {
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
    controller: Arc<Mutex<ComputerController>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut request = format!("ws://127.0.0.1:{port}/computer").into_client_request()?;
    request
        .headers_mut()
        .insert("Origin", COMPUTER_HELPER_ORIGIN.parse()?);
    let (mut socket, _) = connect_async(request).await?;
    let session_id = authenticate_bridge(&mut socket, token).await?;
    let authority = SessionAuthorityGuard::new(Arc::clone(&controller));
    println!("Authenticated Local Browser Bridge.");
    let mut hello = controller
        .lock()
        .map_err(|_| std::io::Error::other("Computer controller lock was poisoned"))?
        .hello();
    if let Some(object) = hello.as_object_mut() {
        object.insert("protocolVersion".to_owned(), json!(PROTOCOL_VERSION));
        object.insert("sessionId".to_owned(), json!(session_id));
    }
    socket.send(Message::Text(hello.to_string().into())).await?;
    let hello_deadline = tokio::time::Instant::now() + AUTH_TIMEOUT;
    for _ in 0..MAX_AUTH_MESSAGES {
        let message = tokio::time::timeout_at(hello_deadline, socket.next())
            .await
            .map_err(|_| auth_io_error("Bridge hello acknowledgement timed out"))?
            .ok_or_else(|| {
                auth_io_error("Bridge closed before accepting the helper handshake")
            })??;
        match message {
            Message::Text(text) => {
                if text.len() > MAX_AUTH_MESSAGE_BYTES {
                    return Err(auth_io_error("Bridge hello acknowledgement was too large").into());
                }
                let message: Value = serde_json::from_str(text.as_str())?;
                if message.get("type").and_then(Value::as_str) != Some("helloAck") {
                    return Err(auth_io_error("Bridge sent an unexpected hello response").into());
                }
                let accepted = message.get("ok").and_then(Value::as_bool) == Some(true)
                    && message.get("protocolVersion").and_then(Value::as_u64)
                        == Some(PROTOCOL_VERSION)
                    && message.get("sessionId").and_then(Value::as_str)
                        == Some(session_id.as_str());
                if !accepted {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "Bridge rejected the computer helper handshake",
                    )
                    .into());
                }
                return run_authenticated_session(socket, session_id, controller, authority).await;
            }
            Message::Ping(bytes) => socket.send(Message::Pong(bytes)).await?,
            Message::Pong(_) => {}
            Message::Close(_) => {
                return Err(
                    auth_io_error("Bridge closed before accepting the helper handshake").into(),
                );
            }
            _ => return Err(auth_io_error("Bridge sent a non-text hello response").into()),
        }
    }
    Err(auth_io_error("Bridge hello acknowledgement message limit exceeded").into())
}

async fn authenticate_bridge<S>(
    socket: &mut WebSocketStream<S>,
    token: &str,
) -> Result<String, Box<dyn std::error::Error>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    let deadline = tokio::time::Instant::now() + AUTH_TIMEOUT;
    let client = ClientHello::new(COMPUTER_CONNECTOR)
        .map_err(|_| auth_io_error("Could not create a fresh authentication hello"))?;
    tokio::time::timeout_at(
        deadline,
        socket.send(Message::Text(client.envelope().to_string().into())),
    )
    .await
    .map_err(|_| auth_io_error("Bridge authentication hello timed out"))??;
    let mut authenticated_session: Option<String> = None;
    for _ in 0..MAX_AUTH_MESSAGES {
        let message = tokio::time::timeout_at(deadline, socket.next())
            .await
            .map_err(|_| auth_io_error("Bridge authentication timed out"))?
            .ok_or_else(|| auth_io_error("Bridge closed during authentication"))??;
        match message {
            Message::Text(text) => {
                if text.len() > MAX_AUTH_MESSAGE_BYTES {
                    return Err(auth_io_error("Bridge authentication message was too large").into());
                }
                let message: Value = serde_json::from_str(text.as_str())?;
                if let Some(session_id) = authenticated_session.as_ref() {
                    let protocol = message.get("protocolVersion").and_then(Value::as_u64);
                    let server_version = message.get("serverVersion").and_then(Value::as_str);
                    let connector = message.get("connector").and_then(Value::as_str);
                    let welcomed_session = message.get("sessionId").and_then(Value::as_str);
                    if message.get("type").and_then(Value::as_str) != Some("welcome")
                        || protocol != Some(PROTOCOL_VERSION)
                        || server_version != Some(VERSION)
                        || connector != Some(COMPUTER_CONNECTOR)
                        || welcomed_session != Some(session_id.as_str())
                    {
                        return Err(
                            auth_io_error("Authenticated bridge welcome was incompatible").into(),
                        );
                    }
                    return Ok(session_id.clone());
                }

                let (session_id, response) = client
                    .answer_challenge(token, &message)
                    .map_err(|_| auth_io_error("Bridge server proof did not verify"))?;
                tokio::time::timeout_at(
                    deadline,
                    socket.send(Message::Text(response.to_string().into())),
                )
                .await
                .map_err(|_| auth_io_error("Bridge authentication response timed out"))??;
                authenticated_session = Some(session_id.to_string());
            }
            Message::Ping(bytes) => {
                tokio::time::timeout_at(deadline, socket.send(Message::Pong(bytes)))
                    .await
                    .map_err(|_| auth_io_error("Bridge authentication pong timed out"))??;
            }
            Message::Pong(_) => {}
            Message::Close(_) => {
                return Err(auth_io_error("Bridge closed during authentication").into());
            }
            _ => return Err(auth_io_error("Bridge sent a non-text authentication message").into()),
        }
    }
    Err(auth_io_error("Bridge authentication message limit exceeded").into())
}

fn auth_io_error(message: &'static str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::PermissionDenied, message)
}

async fn run_authenticated_session(
    socket: WebSocketStream<impl AsyncRead + AsyncWrite + Unpin>,
    session_id: String,
    controller: Arc<Mutex<ComputerController>>,
    mut authority: SessionAuthorityGuard,
) -> Result<(), Box<dyn std::error::Error>> {
    let (mut writer, mut reader) = socket.split();
    let mut share_tick = tokio::time::interval(Duration::from_millis(25));
    share_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut event_sequence = 0_u64;
    let mut last_command_sequence = 0_u64;
    let mut pending = VecDeque::new();
    let mut active: Option<ActiveCommand> = None;
    let (completion_tx, mut completion_rx) = mpsc::unbounded_channel();
    loop {
        tokio::select! {
            biased;
            message = reader.next() => {
                let Some(message) = message else { break };
                match message? {
                    Message::Text(text) => {
                        let Ok(message) = serde_json::from_str::<Value>(text.as_str()) else {
                            continue;
                        };
                        let message_type = message.get("type").and_then(Value::as_str);
                        if message_type == Some("ping") {
                            if !session_message_valid(&message, &session_id) {
                                return Err(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    "Bridge ping used a mismatched protocol session",
                                ).into());
                            }
                            writer
                                .send(Message::Text(json!({
                                    "type": "pong",
                                    "protocolVersion": PROTOCOL_VERSION,
                                    "sessionId": session_id
                                }).to_string().into()))
                                .await?;
                            continue;
                        }
                        if message_type == Some("cancel") {
                            if !session_message_valid(&message, &session_id) {
                                return Err(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    "Bridge cancel used a mismatched protocol session",
                                ).into());
                            }
                            if let Some(key) = command_key(&message) {
                                authority.cancel_exact(&key);
                            }
                            continue;
                        }
                        let sequence = message.get("sequence").and_then(Value::as_u64);
                        if message_type != Some("command")
                            || !session_message_valid(&message, &session_id)
                            || sequence.is_none()
                            || sequence.unwrap() <= last_command_sequence
                        {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                "Bridge command used a stale or mismatched protocol envelope",
                            ).into());
                        }
                        last_command_sequence = sequence.unwrap();
                        let Some((id, method, params)) = command_parts(&message) else {
                            continue;
                        };
                        if pending.len() >= 16 {
                            let mut response = result_envelope(
                                id,
                                Err(ComputerError::new(
                                    "COMPUTER_OVERLOADED",
                                    "Computer helper command queue is full",
                                )),
                            );
                            bind_result_envelope(
                                &mut response,
                                &session_id,
                                last_command_sequence,
                            );
                            writer
                                .send(Message::Text(response.to_string().into()))
                                .await?;
                            continue;
                        }
                        let key = CommandKey {
                            id: id.to_owned(),
                            sequence: last_command_sequence,
                        };
                        let cancellation = CommandCancellation::new();
                        authority.register(key.clone(), cancellation.clone());
                        pending.push_back(QueuedCommand {
                            key,
                            method: method.to_owned(),
                            params,
                            cancellation,
                        });
                        dispatch_next_command(
                            &controller,
                            &mut pending,
                            &mut active,
                            &completion_tx,
                        );
                    }
                    Message::Ping(bytes) => writer.send(Message::Pong(bytes)).await?,
                    Message::Close(_) => break,
                    _ => {}
                }
            }
            completion = completion_rx.recv(), if active.is_some() => {
                let Some(mut completion) = completion else { break };
                let Some(finished) = active.take() else { continue };
                if finished.key != completion.key {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "Computer worker returned a mismatched command identity",
                    ).into());
                }
                completion.result = finished.cancellation.finish(completion.result);
                if finished.method == "computer.share.start"
                    && finished.cancellation.was_dispatched()
                    && finished.cancellation.is_canceled()
                {
                    controller
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner())
                        .reset_transport_session();
                }
                authority.retire(&completion.key);
                let mut response = result_envelope(&completion.key.id, completion.result);
                bind_result_envelope(&mut response, &session_id, completion.key.sequence);
                writer
                    .send(Message::Text(response.to_string().into()))
                    .await?;
                dispatch_next_command(
                    &controller,
                    &mut pending,
                    &mut active,
                    &completion_tx,
                );
            }
            _ = share_tick.tick() => {
                if active.is_some() || !pending.is_empty() {
                    continue;
                }
                let frame = controller
                    .try_lock()
                    .ok()
                    .and_then(|mut controller| controller.next_share_frame());
                if let Some(frame) = frame {
                    event_sequence = event_sequence.saturating_add(1);
                    let mut message = match frame {
                        Ok(frame) => json!({ "type": "event", "name": "computer.share.frame", "data": frame }),
                        Err(error) => json!({
                            "type": "event",
                            "name": "computer.share.error",
                            "data": { "code": error.code, "message": error.message }
                        }),
                    };
                    if let Some(object) = message.as_object_mut() {
                        object.insert("protocolVersion".to_owned(), json!(PROTOCOL_VERSION));
                        object.insert("sessionId".to_owned(), json!(session_id));
                        object.insert("eventSequence".to_owned(), json!(event_sequence));
                    }
                    writer.send(Message::Text(message.to_string().into())).await?;
                }
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct CommandKey {
    id: String,
    sequence: u64,
}

struct QueuedCommand {
    key: CommandKey,
    method: String,
    params: Value,
    cancellation: CommandCancellation,
}

struct ActiveCommand {
    key: CommandKey,
    method: String,
    cancellation: CommandCancellation,
}

struct CommandCompletion {
    key: CommandKey,
    result: Result<Value, ComputerError>,
}

fn command_key(message: &Value) -> Option<CommandKey> {
    Some(CommandKey {
        id: message.get("id")?.as_str()?.to_owned(),
        sequence: message.get("sequence")?.as_u64()?,
    })
}

struct SessionAuthorityGuard {
    controller: Arc<Mutex<ComputerController>>,
    commands: HashMap<CommandKey, CommandCancellation>,
}

impl SessionAuthorityGuard {
    fn new(controller: Arc<Mutex<ComputerController>>) -> Self {
        controller
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .reset_transport_session();
        Self {
            controller,
            commands: HashMap::new(),
        }
    }

    fn register(&mut self, key: CommandKey, cancellation: CommandCancellation) {
        if let Some(replaced) = self.commands.insert(key, cancellation) {
            replaced.cancel();
        }
    }

    fn cancel_exact(&self, key: &CommandKey) -> bool {
        let Some(cancellation) = self.commands.get(key) else {
            return false;
        };
        cancellation.cancel();
        true
    }

    fn retire(&mut self, key: &CommandKey) {
        self.commands.remove(key);
    }
}

impl Drop for SessionAuthorityGuard {
    fn drop(&mut self) {
        for cancellation in self.commands.values() {
            cancellation.cancel();
        }
        self.controller
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .reset_transport_session();
    }
}

fn dispatch_next_command(
    controller: &Arc<Mutex<ComputerController>>,
    pending: &mut VecDeque<QueuedCommand>,
    active: &mut Option<ActiveCommand>,
    completion_tx: &mpsc::UnboundedSender<CommandCompletion>,
) {
    if active.is_some() {
        return;
    }
    let Some(command) = pending.pop_front() else {
        return;
    };
    *active = Some(ActiveCommand {
        key: command.key.clone(),
        method: command.method.clone(),
        cancellation: command.cancellation.clone(),
    });
    let controller = Arc::clone(controller);
    let completion_tx = completion_tx.clone();
    tokio::task::spawn_blocking(move || {
        let result = match controller.lock() {
            Ok(mut controller) => controller.execute_cancellable(
                &command.method,
                &command.params,
                &command.cancellation,
            ),
            Err(_) => Err(ComputerError::new(
                "COMPUTER_HELPER_FAILED",
                "Computer controller lock was poisoned",
            )),
        };
        let _ = completion_tx.send(CommandCompletion {
            key: command.key,
            result,
        });
    });
}

fn bind_result_envelope(response: &mut Value, session_id: &str, sequence: u64) {
    if let Some(object) = response.as_object_mut() {
        object.insert("protocolVersion".to_owned(), json!(PROTOCOL_VERSION));
        object.insert("sessionId".to_owned(), json!(session_id));
        object.insert("sequence".to_owned(), json!(sequence));
    }
}

fn session_message_valid(message: &Value, session_id: &str) -> bool {
    message.get("protocolVersion").and_then(Value::as_u64) == Some(PROTOCOL_VERSION)
        && message.get("sessionId").and_then(Value::as_str) == Some(session_id)
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
    use local_browser_bridge::create_token;
    use local_browser_bridge::ws_auth::{ClientHello, ServerChallenge};
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_hdr_async;

    #[test]
    fn parses_helper_flags_and_ports() {
        let cli = parse_args(["--benchmark".to_owned()].into_iter()).unwrap();
        assert!(cli.benchmark);
        assert_eq!(parse_port(None).unwrap(), 17_373);
        assert!(parse_port(Some("0")).is_err());
        assert!(parse_args(["--unknown".to_owned()].into_iter()).is_err());
    }

    #[tokio::test]
    async fn cancel_before_dispatch_is_bound_to_the_exact_id_and_sequence() {
        let controller = Arc::new(Mutex::new(ComputerController::new()));
        let mut authority = SessionAuthorityGuard::new(Arc::clone(&controller));
        let cancellation = CommandCancellation::new();
        let key = CommandKey {
            id: "command-1".to_owned(),
            sequence: 7,
        };
        authority.register(key.clone(), cancellation.clone());
        let mut pending = VecDeque::from([QueuedCommand {
            key: key.clone(),
            method: "computer.status".to_owned(),
            params: json!({}),
            cancellation: cancellation.clone(),
        }]);
        let mut active = None;
        assert!(!authority.cancel_exact(&CommandKey {
            id: "command-1".to_owned(),
            sequence: 8,
        }));
        assert!(!cancellation.is_canceled());
        assert!(authority.cancel_exact(&key));

        let (completion_tx, mut completion_rx) = mpsc::unbounded_channel();
        dispatch_next_command(&controller, &mut pending, &mut active, &completion_tx);
        let completion = tokio::time::timeout(Duration::from_secs(1), completion_rx.recv())
            .await
            .unwrap()
            .unwrap();
        authority.retire(&completion.key);
        let error = completion.result.unwrap_err();
        assert_eq!(error.code, "COMPUTER_CANCELED");
        assert!(!cancellation.was_dispatched());
    }

    #[test]
    fn dropping_session_authority_cancels_every_registered_command() {
        let controller = Arc::new(Mutex::new(ComputerController::new()));
        let active_cancellation = CommandCancellation::new();
        let queued_cancellation = CommandCancellation::new();
        {
            let mut authority = SessionAuthorityGuard::new(Arc::clone(&controller));
            authority.register(
                CommandKey {
                    id: "active-command".to_owned(),
                    sequence: 11,
                },
                active_cancellation.clone(),
            );
            authority.register(
                CommandKey {
                    id: "queued-command".to_owned(),
                    sequence: 12,
                },
                queued_cancellation.clone(),
            );
        }
        assert!(active_cancellation.is_canceled());
        assert!(queued_cancellation.is_canceled());
        let error = controller
            .lock()
            .unwrap()
            .execute_cancellable("computer.status", &json!({}), &active_cancellation)
            .unwrap_err();
        assert_eq!(error.code, "COMPUTER_CANCELED");
    }

    #[tokio::test]
    async fn rogue_listener_learns_no_secret_and_cannot_replay_a_server_challenge() {
        #[allow(clippy::result_large_err)]
        fn assert_token_free_upgrade(
            request: &tokio_tungstenite::tungstenite::handshake::server::Request,
            response: tokio_tungstenite::tungstenite::handshake::server::Response,
        ) -> Result<
            tokio_tungstenite::tungstenite::handshake::server::Response,
            tokio_tungstenite::tungstenite::handshake::server::ErrorResponse,
        > {
            assert_eq!(request.uri().path(), "/computer");
            assert!(request.uri().query().is_none());
            assert!(!request.headers().contains_key("authorization"));
            Ok(response)
        }

        let token = create_token();
        let stale_client = ClientHello::new(COMPUTER_CONNECTOR).unwrap();
        let stale_challenge =
            ServerChallenge::from_client_hello(&token, COMPUTER_CONNECTOR, stale_client.envelope())
                .unwrap()
                .envelope()
                .to_string();
        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let rogue = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = accept_hdr_async(stream, assert_token_free_upgrade)
                .await
                .unwrap();
            let Message::Text(hello) = socket.next().await.unwrap().unwrap() else {
                panic!("expected token-free auth hello");
            };
            let hello: Value = serde_json::from_str(hello.as_str()).unwrap();
            assert_eq!(hello["type"], "authHello");
            let stale: Value = serde_json::from_str(&stale_challenge).unwrap();
            assert_ne!(hello["clientNonce"], stale["clientNonce"]);
            socket
                .send(Message::Text(stale_challenge.into()))
                .await
                .unwrap();
            if let Ok(Some(Ok(Message::Text(message)))) =
                tokio::time::timeout(Duration::from_millis(500), socket.next()).await
            {
                let message: Value = serde_json::from_str(message.as_str()).unwrap();
                assert_ne!(message["type"], "authResponse");
            }
        });

        let controller = Arc::new(Mutex::new(ComputerController::new()));
        let error = run_session(port, &token, controller).await.unwrap_err();
        assert!(error.to_string().contains("server proof did not verify"));
        rogue.await.unwrap();
    }
}

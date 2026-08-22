use std::cell::Cell;
use std::collections::{HashMap, VecDeque};
use std::env;
#[cfg(target_os = "windows")]
use std::ffi::c_void;
#[cfg(target_os = "windows")]
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
#[cfg(target_os = "windows")]
use std::time::Instant;

use futures_util::{SinkExt as _, StreamExt as _};
use local_browser_bridge::computer::{
    COMPUTER_HELPER_ORIGIN, CommandCancellation, ComputerController, ComputerError,
    NATIVE_COMPUTER_SUPPORTED, ShareFrameAck, command_parts, result_envelope,
};
use local_browser_bridge::ws_auth::{
    AUTH_TIMEOUT, COMPUTER_CONNECTOR, ClientHello, MAX_AUTH_MESSAGE_BYTES, MAX_AUTH_MESSAGES,
};
use local_browser_bridge::{
    PROTOCOL_VERSION, VERSION, default_token_path, load_or_create_token, print_license_report,
};
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest as _;
use tokio_tungstenite::{WebSocketStream, connect_async};

const COMMAND_WATCHDOG_TIMEOUT: Duration = Duration::from_secs(12);
const FATAL_REPORT_TIMEOUT: Duration = Duration::from_millis(250);
const FATAL_CAPTURE_STOP_CODE: &str = "COMPUTER_CAPTURE_STOP_FATAL";
const FATAL_CAPTURE_STOP_DETAIL: &str = "COMPUTER_CAPTURE_STOP_FATAL:";
const WATCHDOG_EXIT_CODE: i32 = 70;
const CAPTURE_STOP_EXIT_CODE: i32 = 71;
#[cfg(target_os = "windows")]
const TRANSPORT_EXIT_CODE: i32 = 72;
const OUTCOME_UNKNOWN_EXIT_CODE: i32 = 73;

#[cfg(target_os = "windows")]
const SUPERVISOR_POLL_INTERVAL: Duration = Duration::from_millis(100);
#[cfg(target_os = "windows")]
const SUPERVISOR_INITIAL_BACKOFF: Duration = Duration::from_millis(250);
#[cfg(target_os = "windows")]
const SUPERVISOR_MAX_BACKOFF: Duration = Duration::from_secs(5);
#[cfg(target_os = "windows")]
const SUPERVISOR_STABLE_RUN: Duration = Duration::from_secs(30);

#[cfg(target_os = "windows")]
const JOB_OBJECT_EXTENDED_LIMIT_INFORMATION: i32 = 9;
#[cfg(target_os = "windows")]
const JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE: u32 = 0x0000_2000;
#[cfg(target_os = "windows")]
const CREATE_SUSPENDED: u32 = 0x0000_0004;
#[cfg(target_os = "windows")]
const WAIT_OBJECT_0: u32 = 0;
#[cfg(target_os = "windows")]
const WAIT_TIMEOUT: u32 = 258;
#[cfg(target_os = "windows")]
const WAIT_FAILED: u32 = u32::MAX;
#[cfg(target_os = "windows")]
const PROCESS_TERMINATION_TIMEOUT_MS: u32 = 5_000;
#[cfg(target_os = "windows")]
const EVENT_MODIFY_STATE: u32 = 0x0002;
#[cfg(target_os = "windows")]
const SYNCHRONIZE: u32 = 0x0010_0000;

#[cfg(target_os = "windows")]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateJobObjectW(attributes: *const c_void, name: *const u16) -> *mut c_void;
    fn SetInformationJobObject(
        job: *mut c_void,
        information_class: i32,
        information: *const c_void,
        information_length: u32,
    ) -> i32;
    fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
    fn CreateEventW(
        event_attributes: *const c_void,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> *mut c_void;
    fn OpenEventW(desired_access: u32, inherit_handle: i32, name: *const u16) -> *mut c_void;
    fn SetEvent(event: *mut c_void) -> i32;
    fn CreateProcessW(
        application_name: *const u16,
        command_line: *mut u16,
        process_attributes: *const c_void,
        thread_attributes: *const c_void,
        inherit_handles: i32,
        creation_flags: u32,
        environment: *const c_void,
        current_directory: *const u16,
        startup_info: *const StartupInfoW,
        process_information: *mut ProcessInformation,
    ) -> i32;
    fn ResumeThread(thread: *mut c_void) -> u32;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn GetExitCodeProcess(process: *mut c_void, exit_code: *mut u32) -> i32;
    fn TerminateProcess(process: *mut c_void, exit_code: u32) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Default)]
struct JobBasicLimitInformation {
    per_process_user_time_limit: i64,
    per_job_user_time_limit: i64,
    limit_flags: u32,
    minimum_working_set_size: usize,
    maximum_working_set_size: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Default)]
struct IoCounters {
    read_operation_count: u64,
    write_operation_count: u64,
    other_operation_count: u64,
    read_transfer_count: u64,
    write_transfer_count: u64,
    other_transfer_count: u64,
}

#[cfg(target_os = "windows")]
#[repr(C)]
#[derive(Default)]
struct JobExtendedLimitInformation {
    basic_limit_information: JobBasicLimitInformation,
    io_info: IoCounters,
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory_used: usize,
    peak_job_memory_used: usize,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct StartupInfoW {
    cb: u32,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    x: u32,
    y: u32,
    x_size: u32,
    y_size: u32,
    x_count_chars: u32,
    y_count_chars: u32,
    fill_attribute: u32,
    flags: u32,
    show_window: u16,
    reserved_size: u16,
    reserved_bytes: *mut u8,
    standard_input: *mut c_void,
    standard_output: *mut c_void,
    standard_error: *mut c_void,
}

#[cfg(target_os = "windows")]
#[repr(C)]
struct ProcessInformation {
    process: *mut c_void,
    thread: *mut c_void,
    process_id: u32,
    thread_id: u32,
}

#[derive(Default)]
struct Cli {
    show_help: bool,
    show_version: bool,
    show_licenses: bool,
    request_permissions: bool,
    benchmark: bool,
    #[cfg(target_os = "windows")]
    worker: bool,
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
    if cli.show_licenses {
        print_license_report("Local Computer Helper");
        return Ok(());
    }
    if !NATIVE_COMPUTER_SUPPORTED {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Native computer control is available only on macOS and Windows",
        )
        .into());
    }

    #[cfg(target_os = "windows")]
    if !cli.worker && !cli.request_permissions && !cli.benchmark {
        return supervise_worker().await;
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

    run_worker(controller).await
}

async fn run_worker(controller: ComputerController) -> Result<(), Box<dyn std::error::Error>> {
    let port = parse_port(env::var("LBB_PORT").ok().as_deref())?;
    let token = match env::var("LBB_TOKEN").ok() {
        Some(token) if !token.trim().is_empty() => token.trim().to_owned(),
        _ => {
            let token_path = match env::var_os("LBB_TOKEN_PATH") {
                Some(path) => PathBuf::from(path),
                None => default_token_path()?,
            };
            load_or_create_token(&token_path).await?
        }
    };

    println!("Local Computer Helper {VERSION}");
    println!("Non-interrupting background-window provider for Local Browser Bridge");
    println!("Connecting to 127.0.0.1:{port}; press Ctrl+C to stop.");
    println!("No global HID input or implicit foreground fallback is used.");
    println!(
        "No shell, filesystem, clipboard, process-launch, or telemetry capability is exposed."
    );

    let controller = Arc::new(Mutex::new(controller));
    #[cfg(target_os = "windows")]
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            println!("Stopping...");
            terminate_worker(0);
        }
        result = run_session(port, &token, Arc::clone(&controller)) => {
            match result {
                Ok(()) => eprintln!("Bridge connection closed; restarting the disposable worker."),
                Err(error) => eprintln!("Bridge session ended: {error}; restarting the disposable worker."),
            }
            terminate_worker(TRANSPORT_EXIT_CODE);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
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
}

#[cfg(target_os = "windows")]
async fn supervise_worker() -> Result<(), Box<dyn std::error::Error>> {
    let mut backoff = SUPERVISOR_INITIAL_BACKOFF;
    // The acceptance-only event is owned by the supervisor so its signaled
    // state survives the deliberately terminated first worker. It is selected
    // only by launch-time environment, never by the bridge protocol.
    let _test_share_pump_fault = TestSharePumpFaultEvent::create_from_environment()?;

    println!("Local Computer Helper {VERSION}");
    println!("Supervising the disposable computer-control worker; press Ctrl+C to stop.");

    loop {
        let mut child = match spawn_worker() {
            Ok(child) => child,
            Err(error) => {
                eprintln!("Could not start the computer-control worker: {error}");
                if wait_for_restart(backoff).await {
                    println!("Stopping...");
                    return Ok(());
                }
                backoff = (backoff * 2).min(SUPERVISOR_MAX_BACKOFF);
                continue;
            }
        };
        let started_at = Instant::now();

        let status = loop {
            tokio::select! {
                biased;
                signal = tokio::signal::ctrl_c() => {
                    signal?;
                    child.terminate();
                    println!("Stopping...");
                    return Ok(());
                }
                _ = tokio::time::sleep(SUPERVISOR_POLL_INTERVAL) => {
                    if let Some(status) = child.try_wait()? {
                        break status;
                    }
                }
            }
        };

        let runtime = started_at.elapsed();
        eprintln!("Computer-control worker exited with {status}; restarting.");
        if runtime >= SUPERVISOR_STABLE_RUN {
            backoff = SUPERVISOR_INITIAL_BACKOFF;
        }
        if wait_for_restart(backoff).await {
            println!("Stopping...");
            return Ok(());
        }
        backoff = (backoff * 2).min(SUPERVISOR_MAX_BACKOFF);
    }
}

#[cfg(target_os = "windows")]
fn spawn_worker() -> std::io::Result<SupervisedWorker> {
    let job = KillOnCloseJob::new()?;
    let mut child = SuspendedWorkerProcess::create()?;
    if let Err(error) = job.assign(&child) {
        child.terminate();
        return Err(error);
    }
    if let Err(error) = child.resume() {
        child.terminate();
        return Err(error);
    }
    Ok(SupervisedWorker::new(child, job))
}

#[cfg(target_os = "windows")]
async fn wait_for_restart(backoff: Duration) -> bool {
    tokio::select! {
        biased;
        _ = tokio::signal::ctrl_c() => true,
        _ = tokio::time::sleep(backoff) => false,
    }
}

#[cfg(target_os = "windows")]
struct SupervisedWorker {
    child: Option<SuspendedWorkerProcess>,
    job: Option<KillOnCloseJob>,
}

#[cfg(target_os = "windows")]
impl SupervisedWorker {
    fn new(child: SuspendedWorkerProcess, job: KillOnCloseJob) -> Self {
        Self {
            child: Some(child),
            job: Some(job),
        }
    }

    fn try_wait(&mut self) -> std::io::Result<Option<u32>> {
        self.child
            .as_mut()
            .expect("supervised child is present")
            .try_wait()
    }

    fn terminate(&mut self) {
        // Closing the supervisor's last job handle terminates the worker (and
        // any descendants) even if the ordinary process handle is stale.
        drop(self.job.take());
        let Some(mut child) = self.child.take() else {
            return;
        };
        child.terminate();
    }
}

#[cfg(target_os = "windows")]
struct KillOnCloseJob {
    handle: usize,
}

#[cfg(target_os = "windows")]
impl KillOnCloseJob {
    fn new() -> std::io::Result<Self> {
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let job = Self {
            handle: handle as usize,
        };
        let limits = kill_on_close_limits();
        let information_length = u32::try_from(std::mem::size_of_val(&limits))
            .expect("Windows job information size fits in u32");
        let configured = unsafe {
            SetInformationJobObject(
                job.raw(),
                JOB_OBJECT_EXTENDED_LIMIT_INFORMATION,
                std::ptr::from_ref(&limits).cast(),
                information_length,
            )
        };
        if configured == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(job)
    }

    fn assign(&self, child: &SuspendedWorkerProcess) -> std::io::Result<()> {
        let assigned = unsafe { AssignProcessToJobObject(self.raw(), child.raw_process()) };
        if assigned == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn raw(&self) -> *mut c_void {
        self.handle as *mut c_void
    }
}

#[cfg(target_os = "windows")]
impl Drop for KillOnCloseJob {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.raw()) };
    }
}

#[cfg(target_os = "windows")]
struct TestSharePumpFaultEvent {
    handle: usize,
}

#[cfg(target_os = "windows")]
impl TestSharePumpFaultEvent {
    fn create_from_environment() -> std::io::Result<Option<Self>> {
        let Some(name) = test_share_pump_fault_event_name() else {
            return Ok(None);
        };
        let handle = unsafe { CreateEventW(std::ptr::null(), 1, 0, name.as_ptr()) };
        if handle.is_null() {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(Some(Self {
                handle: handle as usize,
            }))
        }
    }

    fn raw(&self) -> *mut c_void {
        self.handle as *mut c_void
    }
}

#[cfg(target_os = "windows")]
impl Drop for TestSharePumpFaultEvent {
    fn drop(&mut self) {
        let _ = unsafe { CloseHandle(self.raw()) };
    }
}

#[cfg(target_os = "windows")]
fn kill_on_close_limits() -> JobExtendedLimitInformation {
    let mut limits = JobExtendedLimitInformation::default();
    limits.basic_limit_information.limit_flags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    limits
}

#[cfg(target_os = "windows")]
struct SuspendedWorkerProcess {
    process: usize,
    thread: Option<usize>,
}

#[cfg(target_os = "windows")]
impl SuspendedWorkerProcess {
    fn create() -> std::io::Result<Self> {
        let executable = env::current_exe()?;
        let executable_wide: Vec<u16> = executable.as_os_str().encode_wide().collect();
        if executable_wide.contains(&0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Current executable path contains an embedded NUL",
            ));
        }

        let mut application_name = executable_wide.clone();
        application_name.push(0);
        let mut command_line = Vec::with_capacity(executable_wide.len() + 13);
        command_line.push(u16::from(b'"'));
        command_line.extend(executable_wide);
        command_line.extend("\" --worker".encode_utf16());
        command_line.push(0);

        let mut startup: StartupInfoW = unsafe { std::mem::zeroed() };
        startup.cb = u32::try_from(std::mem::size_of::<StartupInfoW>())
            .expect("Windows startup information size fits in u32");
        let mut process: ProcessInformation = unsafe { std::mem::zeroed() };
        let created = unsafe {
            CreateProcessW(
                application_name.as_ptr(),
                command_line.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                CREATE_SUSPENDED,
                std::ptr::null(),
                std::ptr::null(),
                &startup,
                &mut process,
            )
        };
        if created == 0 {
            return Err(std::io::Error::last_os_error());
        }
        if process.process.is_null() || process.thread.is_null() {
            if !process.thread.is_null() {
                let _ = unsafe { CloseHandle(process.thread) };
            }
            if !process.process.is_null() {
                let _ = unsafe { TerminateProcess(process.process, TRANSPORT_EXIT_CODE as u32) };
                let _ = unsafe { CloseHandle(process.process) };
            }
            return Err(std::io::Error::other(
                "Windows returned incomplete worker process handles",
            ));
        }

        Ok(Self {
            process: process.process as usize,
            thread: Some(process.thread as usize),
        })
    }

    fn resume(&mut self) -> std::io::Result<()> {
        let thread = self
            .thread
            .take()
            .expect("new Windows worker has a suspended primary thread");
        let resumed = unsafe { ResumeThread(thread as *mut c_void) };
        let error = (resumed == u32::MAX).then(std::io::Error::last_os_error);
        let _ = unsafe { CloseHandle(thread as *mut c_void) };
        match error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn try_wait(&self) -> std::io::Result<Option<u32>> {
        match unsafe { WaitForSingleObject(self.raw_process(), 0) } {
            WAIT_TIMEOUT => Ok(None),
            WAIT_OBJECT_0 => {
                let mut exit_code = 0;
                let read = unsafe { GetExitCodeProcess(self.raw_process(), &mut exit_code) };
                if read == 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(Some(exit_code))
                }
            }
            WAIT_FAILED => Err(std::io::Error::last_os_error()),
            status => Err(std::io::Error::other(format!(
                "Unexpected Windows worker wait result: {status}"
            ))),
        }
    }

    fn terminate(&mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = unsafe { CloseHandle(thread as *mut c_void) };
        }
        let _ = unsafe { TerminateProcess(self.raw_process(), TRANSPORT_EXIT_CODE as u32) };
        let _ = unsafe { WaitForSingleObject(self.raw_process(), PROCESS_TERMINATION_TIMEOUT_MS) };
    }

    fn raw_process(&self) -> *mut c_void {
        self.process as *mut c_void
    }
}

#[cfg(target_os = "windows")]
impl Drop for SuspendedWorkerProcess {
    fn drop(&mut self) {
        if self.thread.is_some() {
            // A process that never reached ResumeThread must not escape if an
            // intermediate setup step unwinds before job containment completes.
            self.terminate();
        }
        let _ = unsafe { CloseHandle(self.raw_process()) };
    }
}

#[cfg(target_os = "windows")]
impl Drop for SupervisedWorker {
    fn drop(&mut self) {
        self.terminate();
    }
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
    if let Some(error) = ComputerController::take_fatal_capture_stop_error() {
        eprintln!("{}: {}", error.code, error.message);
        terminate_worker(CAPTURE_STOP_EXIT_CODE);
    }
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
                // The bridge confirms the negotiated share-frame ack pacing in
                // its hello acknowledgement; without the confirmation the
                // helper keeps the legacy timer-only emission behavior.
                let share_ack_paced =
                    message.get("shareAck").and_then(Value::as_bool) == Some(true);
                controller
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .set_share_ack_pacing(share_ack_paced);
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
    let mut pending: VecDeque<QueuedCommand> = VecDeque::new();
    let mut pending_share_acks = VecDeque::new();
    let mut active: Option<ActiveCommand> = None;
    let (completion_tx, mut completion_rx) = mpsc::unbounded_channel();
    let mut active_share_pump: Option<ActiveSharePump> = None;
    let (share_pump_tx, mut share_pump_rx) = mpsc::unbounded_channel::<SharePumpCompletion>();
    loop {
        let command_deadline = active.as_ref().and_then(|command| command.deadline);
        let share_pump_deadline = active_share_pump.as_ref().and_then(|pump| pump.deadline);
        tokio::select! {
            biased;
            _ = wait_for_command_deadline(command_deadline), if command_deadline.is_some() => {
                let finished = active.take().expect("watchdog is armed only for an active command");
                finished.cancellation.cancel();
                authority.cancel_all();
                for command in &pending {
                    command.cancellation.cancel();
                }
                let mut response = result_envelope(
                    &finished.key.id,
                    command_watchdog_result(&finished.cancellation),
                );
                bind_result_envelope(&mut response, &session_id, finished.key.sequence);
                let _ = tokio::time::timeout(
                    FATAL_REPORT_TIMEOUT,
                    writer.send(Message::Text(response.to_string().into())),
                )
                .await;
                eprintln!(
                    "Computer command {} exceeded the hard worker deadline; terminating the disposable worker.",
                    finished.key.id
                );
                terminate_worker(WATCHDOG_EXIT_CODE);
            }
            _ = wait_for_command_deadline(share_pump_deadline), if share_pump_deadline.is_some() => {
                let expired_share = active_share_pump
                    .take()
                    .expect("share watchdog is armed only for an active pump");
                authority.cancel_all();
                for command in &pending {
                    command.cancellation.cancel();
                }
                event_sequence = event_sequence.saturating_add(1);
                let mut message = json!({
                    "type": "event",
                    "name": "computer.share.error",
                    "data": {
                        "code": "COMPUTER_HELPER_WATCHDOG",
                        "message": format!(
                            "The disposable computer worker exceeded its {} second live-share pump deadline and will restart",
                            COMMAND_WATCHDOG_TIMEOUT.as_secs()
                        ),
                        "shareId": expired_share.share_id,
                    }
                });
                bind_event_envelope(
                    &mut message,
                    &session_id,
                    event_sequence,
                );
                let _ = tokio::time::timeout(
                    FATAL_REPORT_TIMEOUT,
                    writer.send(Message::Text(message.to_string().into())),
                )
                .await;
                eprintln!(
                    "The live-share observation/PNG pump exceeded the hard worker deadline; terminating the disposable worker."
                );
                terminate_worker(WATCHDOG_EXIT_CODE);
            }
            completion = share_pump_rx.recv(), if active_share_pump.is_some() => {
                let Some(completion) = completion else { break };
                active_share_pump.take();
                let emissions = completion.emissions;
                let fatal_capture_stop = emissions.iter().any(|(name, data)| {
                    *name == "computer.share.error"
                        && data.get("code").and_then(Value::as_str)
                            == Some(FATAL_CAPTURE_STOP_CODE)
                });
                if fatal_capture_stop {
                    authority.cancel_all();
                    for command in &pending {
                        command.cancellation.cancel();
                    }
                    if let Some((name, data)) = emissions.into_iter().find(|(name, data)| {
                        *name == "computer.share.error"
                            && data.get("code").and_then(Value::as_str)
                                == Some(FATAL_CAPTURE_STOP_CODE)
                    }) {
                        event_sequence = event_sequence.saturating_add(1);
                        let mut message = json!({ "type": "event", "name": name, "data": data });
                        bind_event_envelope(
                            &mut message,
                            &session_id,
                            event_sequence,
                        );
                        let _ = tokio::time::timeout(
                            FATAL_REPORT_TIMEOUT,
                            writer.send(Message::Text(message.to_string().into())),
                        )
                        .await;
                    }
                    eprintln!(
                        "Windows Graphics Capture shutdown could not be confirmed; terminating the disposable worker."
                    );
                    terminate_worker(CAPTURE_STOP_EXIT_CODE);
                }
                for (name, data) in emissions {
                    event_sequence = event_sequence.saturating_add(1);
                    let mut message = json!({ "type": "event", "name": name, "data": data });
                    bind_event_envelope(
                        &mut message,
                        &session_id,
                        event_sequence,
                    );
                    writer.send(Message::Text(message.to_string().into())).await?;
                }
                dispatch_next_command(
                    &controller,
                    &mut pending,
                    &mut active,
                    &completion_tx,
                    false,
                );
            }
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
                        if message_type == Some("eventAck") {
                            if !session_message_valid(&message, &session_id) {
                                return Err(std::io::Error::new(
                                    std::io::ErrorKind::InvalidData,
                                    "Bridge event acknowledgement used a mismatched protocol session",
                                ).into());
                            }
                            // Acks are queued instead of locking the controller
                            // here so the reader stays responsive while a
                            // command holds the controller lock.
                            if let Some(ack) = share_frame_ack(&message) {
                                queue_share_ack(&mut pending_share_acks, ack);
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
                            active_share_pump.is_some(),
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
                if let Some(error) = completion.fatal_capture_stop.take() {
                    completion.result = Err(error);
                }
                let fatal_capture_stop = is_fatal_capture_stop(&completion.result);
                let restart_for_outcome_unknown = Cell::new(false);
                completion.result = finish_worker_result(
                    &finished.cancellation,
                    completion.result,
                    || {
                        if !fatal_capture_stop {
                            #[cfg(target_os = "windows")]
                            restart_for_outcome_unknown.set(true);
                            // Cancellation can arrive after the blocking worker has
                            // returned but before its result is serialized. If that
                            // changes the result to outcome-unknown, revoke the same
                            // authority the controller revokes for an unknown result
                            // detected inside the worker. Ack negotiation belongs to
                            // the still-live transport session and is retained.
                            #[cfg(not(target_os = "windows"))]
                            controller
                                .lock()
                                .unwrap_or_else(|poisoned| poisoned.into_inner())
                                .revoke_command_authority();
                        }
                    },
                );
                authority.retire(&completion.key);
                let fatal_capture_stop =
                    fatal_capture_stop || is_fatal_capture_stop(&completion.result);
                let mut response = result_envelope(&completion.key.id, completion.result);
                bind_result_envelope(&mut response, &session_id, completion.key.sequence);
                if fatal_capture_stop || restart_for_outcome_unknown.get() {
                    authority.cancel_all();
                    for command in &pending {
                        command.cancellation.cancel();
                    }
                    let _ = tokio::time::timeout(
                        FATAL_REPORT_TIMEOUT,
                        writer.send(Message::Text(response.to_string().into())),
                    )
                    .await;
                    let exit_code = if fatal_capture_stop {
                        eprintln!(
                            "Windows Graphics Capture shutdown could not be confirmed; terminating the disposable worker."
                        );
                        CAPTURE_STOP_EXIT_CODE
                    } else {
                        eprintln!(
                            "A computer command ended with an unknown outcome; restarting the disposable worker to revoke authority."
                        );
                        OUTCOME_UNKNOWN_EXIT_CODE
                    };
                    terminate_worker(exit_code);
                }
                writer
                    .send(Message::Text(response.to_string().into()))
                    .await?;
                dispatch_next_command(
                    &controller,
                    &mut pending,
                    &mut active,
                    &completion_tx,
                    false,
                );
            }
            _ = share_tick.tick() => {
                dispatch_share_pump(
                    &controller,
                    &mut pending_share_acks,
                    &mut active_share_pump,
                    &share_pump_tx,
                    active.is_some() || !pending.is_empty(),
                );
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
    cancellation: CommandCancellation,
    deadline: Option<tokio::time::Instant>,
}

struct CommandCompletion {
    key: CommandKey,
    result: Result<Value, ComputerError>,
    fatal_capture_stop: Option<ComputerError>,
}

struct ActiveSharePump {
    deadline: Option<tokio::time::Instant>,
    share_id: Option<String>,
}

struct SharePumpCompletion {
    emissions: Vec<(&'static str, Value)>,
}

fn command_key(message: &Value) -> Option<CommandKey> {
    Some(CommandKey {
        id: message.get("id")?.as_str()?.to_owned(),
        sequence: message.get("sequence")?.as_u64()?,
    })
}

fn share_frame_ack(message: &Value) -> Option<ShareFrameAck> {
    if message.get("name").and_then(Value::as_str) != Some("computer.share.frame") {
        return None;
    }
    let share_id = message.get("shareId")?.as_str()?;
    if share_id.is_empty() || share_id.len() > 100 {
        return None;
    }
    Some(ShareFrameAck {
        share_id: share_id.to_owned(),
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

    fn cancel_all(&self) {
        for cancellation in self.commands.values() {
            cancellation.cancel();
        }
    }
}

impl Drop for SessionAuthorityGuard {
    fn drop(&mut self) {
        for cancellation in self.commands.values() {
            cancellation.cancel();
        }
        // A production Windows worker is disposable: its caller terminates it
        // as soon as this transport future returns. Do not synchronously wait
        // for a controller lock or WGC shutdown here, because either may be the
        // platform call whose failure caused transport teardown.
        #[cfg(all(target_os = "windows", not(test)))]
        let _ = &self.controller;

        #[cfg(any(not(target_os = "windows"), test))]
        self.controller
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .reset_transport_session();
    }
}

/// At most one share-frame ack can be outstanding, so a tiny bound absorbs
/// even a misbehaving bridge without growing an unbounded queue.
const MAX_PENDING_SHARE_ACKS: usize = 8;

fn queue_share_ack(pending_share_acks: &mut VecDeque<ShareFrameAck>, ack: ShareFrameAck) {
    if pending_share_acks.len() >= MAX_PENDING_SHARE_ACKS {
        pending_share_acks.pop_front();
    }
    pending_share_acks.push_back(ack);
}

fn finish_worker_result(
    cancellation: &CommandCancellation,
    result: Result<Value, ComputerError>,
    revoke_authority: impl FnOnce(),
) -> Result<Value, ComputerError> {
    let result = cancellation.finish(result);
    if result
        .as_ref()
        .err()
        .is_some_and(|error| error.code == "COMPUTER_OUTCOME_UNKNOWN")
    {
        revoke_authority();
    }
    result
}

fn is_fatal_capture_stop(result: &Result<Value, ComputerError>) -> bool {
    result.as_ref().err().is_some_and(|error| {
        error.code == FATAL_CAPTURE_STOP_CODE
            || (error.code == "COMPUTER_OUTCOME_UNKNOWN"
                && error.message.contains(FATAL_CAPTURE_STOP_DETAIL))
    })
}

fn command_watchdog_result(cancellation: &CommandCancellation) -> Result<Value, ComputerError> {
    cancellation.finish(Err(ComputerError::new(
        "COMPUTER_HELPER_WATCHDOG",
        format!(
            "The disposable computer worker exceeded its {} second command deadline and will restart",
            COMMAND_WATCHDOG_TIMEOUT.as_secs()
        ),
    )))
}

async fn wait_for_command_deadline(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

fn terminate_worker(exit_code: i32) -> ! {
    std::process::exit(exit_code)
}

fn new_command_deadline() -> Option<tokio::time::Instant> {
    #[cfg(target_os = "windows")]
    {
        Some(tokio::time::Instant::now() + COMMAND_WATCHDOG_TIMEOUT)
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

fn new_share_pump_deadline() -> Option<tokio::time::Instant> {
    new_command_deadline()
}

/// Applies queued share acknowledgements, then drains at most one parked
/// frame and one capture error for emission.
///
/// This contains the platform capture pump, UI Automation snapshot, and PNG
/// conversion. Production calls it only from the serialized `spawn_blocking`
/// share-pump operation, never from the async WebSocket transport future.
fn collect_share_emissions(
    controller: &mut ComputerController,
    pending_share_acks: &mut VecDeque<ShareFrameAck>,
) -> Vec<(&'static str, Value)> {
    let producing_share_id = controller.active_share_id().map(str::to_owned);
    if let Some(error) = ComputerController::take_fatal_capture_stop_error() {
        return vec![(
            "computer.share.error",
            json!({
                "code": error.code,
                "message": error.message,
                "shareId": producing_share_id,
            }),
        )];
    }
    while let Some(ack) = pending_share_acks.pop_front() {
        controller.acknowledge_share_frame(&ack.share_id, ack.sequence);
    }
    let capture_error = controller.pump_share_capture();
    if let Some(error) = ComputerController::take_fatal_capture_stop_error() {
        return vec![(
            "computer.share.error",
            json!({
                "code": error.code,
                "message": error.message,
                "shareId": producing_share_id,
            }),
        )];
    }
    let mut emissions = Vec::new();
    if let Some((_, frame)) = controller.take_share_emission() {
        emissions.push(("computer.share.frame", frame));
    }
    if let Some(error) = capture_error {
        emissions.push((
            "computer.share.error",
            json!({
                "code": error.code,
                "message": error.message,
                "shareId": producing_share_id,
            }),
        ));
    }
    emissions
}

fn dispatch_share_pump(
    controller: &Arc<Mutex<ComputerController>>,
    pending_share_acks: &mut VecDeque<ShareFrameAck>,
    active: &mut Option<ActiveSharePump>,
    completion_tx: &mpsc::UnboundedSender<SharePumpCompletion>,
    command_owns_controller: bool,
) {
    if active.is_some() || command_owns_controller {
        return;
    }
    let share_id = controller
        .lock()
        .ok()
        .and_then(|controller| controller.active_share_id().map(str::to_owned));
    *active = Some(ActiveSharePump {
        deadline: new_share_pump_deadline(),
        share_id: share_id.clone(),
    });
    let controller = Arc::clone(controller);
    let completion_tx = completion_tx.clone();
    let mut pending_share_acks = std::mem::take(pending_share_acks);
    tokio::task::spawn_blocking(move || {
        let emissions = match controller.lock() {
            Ok(mut controller) => {
                if controller.has_active_share() {
                    maybe_stall_share_pump_for_fixture();
                }
                collect_share_emissions(&mut controller, &mut pending_share_acks)
            }
            Err(_) => vec![(
                "computer.share.error",
                json!({
                    "code": "COMPUTER_HELPER_FAILED",
                    "message": "Computer controller lock was poisoned",
                    "shareId": share_id,
                }),
            )],
        };
        let _ = completion_tx.send(SharePumpCompletion { emissions });
    });
}

#[cfg(target_os = "windows")]
const TEST_SHARE_PUMP_STALL_EVENT_ENV: &str = "LBB_TEST_STALL_SHARE_PUMP_ONCE_EVENT";
#[cfg(target_os = "windows")]
const TEST_SHARE_PUMP_STALL_EVENT_PREFIX: &str = "Local\\LBBTestSharePump-";

#[cfg(target_os = "windows")]
fn maybe_stall_share_pump_for_fixture() {
    let Some(name) = test_share_pump_fault_event_name() else {
        return;
    };
    let event = unsafe { OpenEventW(EVENT_MODIFY_STATE | SYNCHRONIZE, 0, name.as_ptr()) };
    if event.is_null() {
        eprintln!(
            "Could not open the acceptance-only share-pump fault event: {}",
            std::io::Error::last_os_error()
        );
        return;
    }
    let wait = unsafe { WaitForSingleObject(event, 0) };
    if wait == WAIT_TIMEOUT {
        if unsafe { SetEvent(event) } == 0 {
            eprintln!(
                "Could not signal the acceptance-only share-pump fault event: {}",
                std::io::Error::last_os_error()
            );
        } else {
            // This is a launch-time, local-DoS-only acceptance hook. The hard
            // share-pump deadline must terminate this disposable process.
            loop {
                std::thread::park();
            }
        }
    } else if wait != WAIT_OBJECT_0 {
        eprintln!("Unexpected acceptance-only fault-event wait result: {wait}");
    }
    let _ = unsafe { CloseHandle(event) };
}

#[cfg(target_os = "windows")]
fn test_share_pump_fault_event_name() -> Option<Vec<u16>> {
    let name = env::var(TEST_SHARE_PUMP_STALL_EVENT_ENV).ok()?;
    if !valid_test_share_pump_fault_event_name(&name) {
        eprintln!(
            "Ignoring invalid {TEST_SHARE_PUMP_STALL_EVENT_ENV}; expected a unique {TEST_SHARE_PUMP_STALL_EVENT_PREFIX}<id> name"
        );
        return None;
    }
    let mut wide: Vec<u16> = name.encode_utf16().collect();
    wide.push(0);
    Some(wide)
}

#[cfg(target_os = "windows")]
fn valid_test_share_pump_fault_event_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix(TEST_SHARE_PUMP_STALL_EVENT_PREFIX) else {
        return false;
    };
    !suffix.is_empty()
        && suffix.len() <= 80
        && suffix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

#[cfg(not(target_os = "windows"))]
fn maybe_stall_share_pump_for_fixture() {}

fn dispatch_next_command(
    controller: &Arc<Mutex<ComputerController>>,
    pending: &mut VecDeque<QueuedCommand>,
    active: &mut Option<ActiveCommand>,
    completion_tx: &mpsc::UnboundedSender<CommandCompletion>,
    share_pump_owns_controller: bool,
) {
    if active.is_some() || share_pump_owns_controller {
        return;
    }
    let Some(command) = pending.pop_front() else {
        return;
    };
    *active = Some(ActiveCommand {
        key: command.key.clone(),
        cancellation: command.cancellation.clone(),
        deadline: new_command_deadline(),
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
        let fatal_capture_stop = ComputerController::take_fatal_capture_stop_error();
        let _ = completion_tx.send(CommandCompletion {
            key: command.key,
            result,
            fatal_capture_stop,
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

fn bind_event_envelope(event: &mut Value, session_id: &str, sequence: u64) {
    if let Some(object) = event.as_object_mut() {
        object.insert("protocolVersion".to_owned(), json!(PROTOCOL_VERSION));
        object.insert("sessionId".to_owned(), json!(session_id));
        object.insert("eventSequence".to_owned(), json!(sequence));
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
            "--licenses" => cli.show_licenses = true,
            "--request-permissions" => cli.request_permissions = true,
            "--benchmark" => cli.benchmark = true,
            #[cfg(target_os = "windows")]
            "--worker" => cli.worker = true,
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
    println!("{}", help_text());
}

fn help_text() -> String {
    format!(
        "Local Computer Helper {VERSION}\n\n\
Usage: local-computer-helper [OPTIONS]\n\n\
Options:\n\
  --request-permissions   Request/check screen-capture and input permissions, then exit\n\
  --benchmark             Benchmark five screen observations, then exit\n\
  --licenses              Print project and third-party license notices, then exit\n\
  -V, --version           Print the installed version and exit\n\
  -h, --help              Print this help\n\n\
Without options, the helper connects to Local Browser Bridge on loopback."
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

#[cfg(test)]
mod tests {
    use super::*;
    use local_browser_bridge::create_token;
    use local_browser_bridge::ws_auth::{ClientHello, ServerChallenge};
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_hdr_async;

    #[test]
    fn share_tick_drains_queued_acks_when_no_share_is_active() {
        let mut controller = ComputerController::new();
        let mut pending_share_acks = VecDeque::from([
            ShareFrameAck {
                share_id: "share-old".to_owned(),
                sequence: 3,
            },
            ShareFrameAck {
                share_id: "share-current".to_owned(),
                sequence: 4,
            },
        ]);
        assert!(collect_share_emissions(&mut controller, &mut pending_share_acks).is_empty());
        assert!(
            pending_share_acks.is_empty(),
            "queued acknowledgements must always be drained when the controller is available"
        );
        assert!(collect_share_emissions(&mut controller, &mut pending_share_acks).is_empty());
    }

    #[test]
    fn queued_share_acks_stay_bounded_and_keep_the_newest_entries() {
        let mut pending_share_acks = VecDeque::new();
        for sequence in 0..(MAX_PENDING_SHARE_ACKS as u64 + 4) {
            queue_share_ack(
                &mut pending_share_acks,
                ShareFrameAck {
                    share_id: format!("share-{sequence}"),
                    sequence,
                },
            );
        }
        assert_eq!(pending_share_acks.len(), MAX_PENDING_SHARE_ACKS);
        assert_eq!(pending_share_acks.front().map(|ack| ack.sequence), Some(4));
        assert_eq!(
            pending_share_acks.back().map(|ack| ack.sequence),
            Some(MAX_PENDING_SHARE_ACKS as u64 + 3)
        );
    }

    #[test]
    fn share_frame_ack_requires_the_exact_event_name_share_id_and_sequence() {
        let ack = share_frame_ack(&json!({
            "name": "computer.share.frame",
            "shareId": "share-1",
            "sequence": 7,
        }))
        .expect("valid share acknowledgement");
        assert_eq!(ack.share_id, "share-1");
        assert_eq!(ack.sequence, 7);
        assert!(
            share_frame_ack(&json!({
                "name": "computer.share.frame",
                "sequence": 7,
            }))
            .is_none()
        );
        assert!(
            share_frame_ack(&json!({
                "name": "computer.share.error",
                "shareId": "share-1",
                "sequence": 7,
            }))
            .is_none()
        );
    }

    #[test]
    fn cancellation_after_worker_return_revokes_authority_before_result_serialization() {
        let cancellation = CommandCancellation::new();
        cancellation
            .begin_side_effect("fixture worker dispatch")
            .expect("fixture dispatch should begin");
        cancellation.cancel();
        let mut revoked = false;

        let error = finish_worker_result(&cancellation, Ok(json!({ "ok": true })), || {
            revoked = true;
        })
        .expect_err("late cancellation must make the outcome unknown");

        assert_eq!(error.code, "COMPUTER_OUTCOME_UNKNOWN");
        assert!(
            revoked,
            "authority must be revoked before returning the error"
        );
    }

    #[test]
    fn command_watchdog_precedes_the_bridge_timeout() {
        assert!(COMMAND_WATCHDOG_TIMEOUT < Duration::from_secs(15));
        assert!(FATAL_REPORT_TIMEOUT < Duration::from_secs(1));
        #[cfg(target_os = "windows")]
        {
            assert!(new_command_deadline().is_some());
            assert!(new_share_pump_deadline().is_some());
        }
        #[cfg(not(target_os = "windows"))]
        {
            assert!(new_command_deadline().is_none());
            assert!(new_share_pump_deadline().is_none());
        }
    }

    #[test]
    fn command_watchdog_preserves_outcome_unknown_after_dispatch() {
        let before_dispatch = CommandCancellation::new();
        before_dispatch.cancel();
        let retry_safe = command_watchdog_result(&before_dispatch).unwrap_err();
        assert_eq!(retry_safe.code, "COMPUTER_HELPER_WATCHDOG");

        let after_dispatch = CommandCancellation::new();
        after_dispatch
            .begin_side_effect("fixture action")
            .expect("fixture action should cross its side-effect boundary");
        after_dispatch.cancel();
        let outcome_unknown = command_watchdog_result(&after_dispatch).unwrap_err();
        assert_eq!(outcome_unknown.code, "COMPUTER_OUTCOME_UNKNOWN");
        assert!(outcome_unknown.message.contains("COMPUTER_HELPER_WATCHDOG"));
    }

    #[test]
    fn fatal_capture_stop_survives_outcome_unknown_wrapping() {
        assert!(is_fatal_capture_stop(&Err(ComputerError::new(
            FATAL_CAPTURE_STOP_CODE,
            "capture stop was not confirmed",
        ))));
        let cancellation = CommandCancellation::new();
        cancellation
            .begin_side_effect("fixture capture stop")
            .expect("fixture stop should cross its side-effect boundary");
        let wrapped = cancellation.finish(Err::<Value, _>(ComputerError::new(
            FATAL_CAPTURE_STOP_CODE,
            "capture stop was not confirmed",
        )));
        assert_eq!(
            wrapped.as_ref().unwrap_err().code,
            "COMPUTER_OUTCOME_UNKNOWN",
        );
        assert!(is_fatal_capture_stop(&wrapped));
        assert!(!is_fatal_capture_stop(&Err(ComputerError::new(
            "COMPUTER_CAPTURE_FAILED",
            "ordinary capture failure",
        ))));
    }

    #[test]
    fn parses_helper_flags_and_ports() {
        let cli = parse_args(["--benchmark".to_owned()].into_iter()).unwrap();
        assert!(cli.benchmark);
        assert!(
            parse_args(["--licenses".to_owned()].into_iter())
                .unwrap()
                .show_licenses
        );
        assert_eq!(parse_port(None).unwrap(), 17_373);
        assert!(parse_port(Some("0")).is_err());
        assert!(parse_args(["--unknown".to_owned()].into_iter()).is_err());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn hidden_worker_flag_is_accepted_but_not_advertised() {
        let cli = parse_args(["--worker".to_owned()].into_iter()).unwrap();
        assert!(cli.worker);
        assert!(!help_text().contains("--worker"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn supervisor_job_contract_is_kill_on_close() {
        let limits = kill_on_close_limits();
        assert_eq!(
            limits.basic_limit_information.limit_flags,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE
        );
        assert!(std::mem::size_of_val(&limits) <= u32::MAX as usize);
        let job =
            KillOnCloseJob::new().expect("Windows must allow an unprivileged kill-on-close job");
        assert!(!job.raw().is_null());
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn share_pump_stall_fault_event_names_are_strictly_local_and_bounded() {
        assert!(valid_test_share_pump_fault_event_name(
            "Local\\LBBTestSharePump-acceptance-123"
        ));
        assert!(!valid_test_share_pump_fault_event_name(
            "Global\\LBBTestSharePump-acceptance-123"
        ));
        assert!(!valid_test_share_pump_fault_event_name(
            "Local\\LBBTestSharePump-..\\escape"
        ));
        assert!(!valid_test_share_pump_fault_event_name(
            "Local\\LBBTestSharePump-"
        ));
    }

    #[tokio::test]
    async fn command_dispatch_waits_for_the_serialized_share_pump() {
        let controller = Arc::new(Mutex::new(ComputerController::new()));
        let cancellation = CommandCancellation::new();
        let mut pending = VecDeque::from([QueuedCommand {
            key: CommandKey {
                id: "queued-behind-share".to_owned(),
                sequence: 1,
            },
            method: "computer.status".to_owned(),
            params: json!({}),
            cancellation: cancellation.clone(),
        }]);
        let mut active = None;
        let (completion_tx, mut completion_rx) = mpsc::unbounded_channel();

        dispatch_next_command(&controller, &mut pending, &mut active, &completion_tx, true);
        assert!(active.is_none());
        assert_eq!(pending.len(), 1);
        assert!(completion_rx.try_recv().is_err());

        cancellation.cancel();
        dispatch_next_command(
            &controller,
            &mut pending,
            &mut active,
            &completion_tx,
            false,
        );
        let completion = tokio::time::timeout(Duration::from_secs(1), completion_rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(completion.key.id, "queued-behind-share");
        assert_eq!(completion.result.unwrap_err().code, "COMPUTER_CANCELED");
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
        dispatch_next_command(
            &controller,
            &mut pending,
            &mut active,
            &completion_tx,
            false,
        );
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

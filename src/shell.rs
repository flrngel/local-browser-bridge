use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncRead, AsyncReadExt};
use tokio::process::{Child, Command};

pub const SHELL_METHODS: &[&str] = &["shell.status", "shell.run"];
pub const DEFAULT_SHELL_TIMEOUT_MS: u64 = 30_000;
pub const MAX_SHELL_TIMEOUT_MS: u64 = 120_000;
pub const MAX_SHELL_COMMAND_BYTES: usize = 16 * 1024;
pub const MAX_SHELL_OUTPUT_BYTES: usize = 1024 * 1024;

#[derive(Clone, Debug)]
pub struct ShellRequest {
    pub command: String,
    pub shell: NativeShell,
    pub cwd: Option<PathBuf>,
    pub timeout: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeShell {
    Default,
    PowerShell,
    Cmd,
    Zsh,
    Sh,
}

impl NativeShell {
    pub fn parse(value: Option<&str>) -> Result<Self, ShellError> {
        match value.unwrap_or("default") {
            "default" => Ok(Self::Default),
            "powershell" => Ok(Self::PowerShell),
            "cmd" => Ok(Self::Cmd),
            "zsh" => Ok(Self::Zsh),
            "sh" => Ok(Self::Sh),
            _ => Err(ShellError::BadRequest(
                "shell must be one of: default, powershell, cmd, zsh, sh".to_owned(),
            )),
        }
    }
}

#[derive(Debug)]
pub enum ShellError {
    BadRequest(String),
    Unsupported(String),
    Spawn(String),
    Wait(String),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellOutput {
    pub shell: &'static str,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u128,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

pub fn status(enabled: bool) -> Value {
    serde_json::json!({
        "enabled": enabled,
        "platform": std::env::consts::OS,
        "defaultShell": default_shell_name(),
        "availableShells": available_shells(),
        "interactive": false,
        "maxCommandBytes": MAX_SHELL_COMMAND_BYTES,
        "maxOutputBytesPerStream": MAX_SHELL_OUTPUT_BYTES,
        "maxTimeoutMs": MAX_SHELL_TIMEOUT_MS,
    })
}

pub fn parse_request(params: &Value) -> Result<ShellRequest, ShellError> {
    let object = params
        .as_object()
        .ok_or_else(|| ShellError::BadRequest("shell.run params must be an object".to_owned()))?;
    let command = object
        .get("command")
        .and_then(Value::as_str)
        .ok_or_else(|| ShellError::BadRequest("command is required".to_owned()))?;
    if command.is_empty() || command.len() > MAX_SHELL_COMMAND_BYTES {
        return Err(ShellError::BadRequest(format!(
            "command must be between 1 and {MAX_SHELL_COMMAND_BYTES} UTF-8 bytes"
        )));
    }
    let shell = NativeShell::parse(object.get("shell").and_then(Value::as_str))?;
    validate_shell(shell)?;
    let timeout_ms = object
        .get("timeoutMs")
        .map(|value| {
            value
                .as_u64()
                .ok_or_else(|| ShellError::BadRequest("timeoutMs must be an integer".to_owned()))
        })
        .transpose()?
        .unwrap_or(DEFAULT_SHELL_TIMEOUT_MS);
    if !(1..=MAX_SHELL_TIMEOUT_MS).contains(&timeout_ms) {
        return Err(ShellError::BadRequest(format!(
            "timeoutMs must be between 1 and {MAX_SHELL_TIMEOUT_MS}"
        )));
    }
    let cwd = object
        .get("cwd")
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .ok_or_else(|| ShellError::BadRequest("cwd must be a non-empty string".to_owned()))
        })
        .transpose()?;
    if let Some(cwd) = cwd.as_ref()
        && !cwd.is_dir()
    {
        return Err(ShellError::BadRequest(
            "cwd must name an existing directory".to_owned(),
        ));
    }
    Ok(ShellRequest {
        command: command.to_owned(),
        shell,
        cwd,
        timeout: Duration::from_millis(timeout_ms),
    })
}

pub async fn run(request: ShellRequest) -> Result<ShellOutput, ShellError> {
    let (program, args, shell_name) = command_line(request.shell, &request.command)?;
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = request.cwd {
        command.current_dir(cwd);
    }
    #[cfg(unix)]
    command.process_group(0);

    let started = Instant::now();
    let mut child = command
        .spawn()
        .map_err(|error| ShellError::Spawn(error.to_string()))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| ShellError::Spawn("stdout pipe was not created".to_owned()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| ShellError::Spawn("stderr pipe was not created".to_owned()))?;
    let stdout_task = tokio::spawn(read_limited(stdout));
    let stderr_task = tokio::spawn(read_limited(stderr));

    let (status, timed_out) = match tokio::time::timeout(request.timeout, child.wait()).await {
        Ok(result) => (
            Some(result.map_err(|error| ShellError::Wait(error.to_string()))?),
            false,
        ),
        Err(_) => {
            terminate_process_tree(&mut child).await;
            let status = child.wait().await.ok();
            (status, true)
        }
    };
    let (stdout, stdout_truncated) = stdout_task
        .await
        .map_err(|error| ShellError::Wait(error.to_string()))?
        .map_err(|error| ShellError::Wait(error.to_string()))?;
    let (stderr, stderr_truncated) = stderr_task
        .await
        .map_err(|error| ShellError::Wait(error.to_string()))?
        .map_err(|error| ShellError::Wait(error.to_string()))?;

    Ok(ShellOutput {
        shell: shell_name,
        exit_code: status.and_then(|status| status.code()),
        timed_out,
        duration_ms: started.elapsed().as_millis(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        stdout_truncated,
        stderr_truncated,
    })
}

async fn read_limited(
    mut reader: impl AsyncRead + Unpin,
) -> Result<(Vec<u8>, bool), std::io::Error> {
    let mut output = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        let remaining = MAX_SHELL_OUTPUT_BYTES.saturating_sub(output.len());
        if remaining > 0 {
            output.extend_from_slice(&buffer[..count.min(remaining)]);
        }
        truncated |= count > remaining;
    }
    Ok((output, truncated))
}

#[cfg(unix)]
async fn terminate_process_tree(child: &mut Child) {
    if let Some(pid) = child.id() {
        // The child is its own process-group leader. SIGKILL closes the entire
        // command tree instead of leaving grandchildren after a timeout.
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
    let _ = child.kill().await;
}

#[cfg(windows)]
async fn terminate_process_tree(child: &mut Child) {
    if let Some(pid) = child.id() {
        let _ = Command::new("taskkill.exe")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await;
    }
    let _ = child.kill().await;
}

#[cfg(not(any(unix, windows)))]
async fn terminate_process_tree(child: &mut Child) {
    let _ = child.kill().await;
}

#[cfg(windows)]
fn default_shell_name() -> &'static str {
    "powershell"
}

#[cfg(target_os = "macos")]
fn default_shell_name() -> &'static str {
    "zsh"
}

#[cfg(not(any(windows, target_os = "macos")))]
fn default_shell_name() -> &'static str {
    "sh"
}

#[cfg(windows)]
fn available_shells() -> &'static [&'static str] {
    &["powershell", "cmd"]
}

#[cfg(target_os = "macos")]
fn available_shells() -> &'static [&'static str] {
    &["zsh", "sh"]
}

#[cfg(not(any(windows, target_os = "macos")))]
fn available_shells() -> &'static [&'static str] {
    &["sh"]
}

fn validate_shell(shell: NativeShell) -> Result<(), ShellError> {
    #[cfg(windows)]
    if matches!(shell, NativeShell::Zsh | NativeShell::Sh) {
        return Err(ShellError::Unsupported(
            "zsh and sh are not supported by the Windows package".to_owned(),
        ));
    }
    #[cfg(target_os = "macos")]
    if matches!(shell, NativeShell::PowerShell | NativeShell::Cmd) {
        return Err(ShellError::Unsupported(
            "powershell and cmd are not supported by the macOS package".to_owned(),
        ));
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    if !matches!(shell, NativeShell::Default | NativeShell::Sh) {
        return Err(ShellError::Unsupported(
            "only sh is supported on this development platform".to_owned(),
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn command_line(
    shell: NativeShell,
    command: &str,
) -> Result<(&'static str, Vec<&str>, &'static str), ShellError> {
    match shell {
        NativeShell::Default | NativeShell::PowerShell => Ok((
            "powershell.exe",
            vec![
                "-NoLogo",
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                command,
            ],
            "powershell",
        )),
        NativeShell::Cmd => Ok(("cmd.exe", vec!["/D", "/S", "/C", command], "cmd")),
        _ => Err(ShellError::Unsupported(
            "unsupported Windows shell".to_owned(),
        )),
    }
}

#[cfg(target_os = "macos")]
fn command_line(
    shell: NativeShell,
    command: &str,
) -> Result<(&'static str, Vec<&str>, &'static str), ShellError> {
    match shell {
        NativeShell::Default | NativeShell::Zsh => {
            Ok(("/bin/zsh", vec!["-f", "-c", command], "zsh"))
        }
        NativeShell::Sh => Ok(("/bin/sh", vec!["-c", command], "sh")),
        _ => Err(ShellError::Unsupported(
            "unsupported macOS shell".to_owned(),
        )),
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
fn command_line(
    shell: NativeShell,
    command: &str,
) -> Result<(&'static str, Vec<&str>, &'static str), ShellError> {
    match shell {
        NativeShell::Default | NativeShell::Sh => Ok(("/bin/sh", vec!["-c", command], "sh")),
        _ => Err(ShellError::Unsupported(
            "unsupported development-platform shell".to_owned(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_request_bounds() {
        assert!(parse_request(&serde_json::json!({ "command": "" })).is_err());
        assert!(
            parse_request(&serde_json::json!({ "command": "echo ok", "timeoutMs": 0 })).is_err()
        );
        assert!(
            parse_request(&serde_json::json!({ "command": "echo ok", "timeoutMs": 1000 })).is_ok()
        );
    }

    #[tokio::test]
    async fn runs_the_native_default_shell() {
        let command = if cfg!(windows) {
            "[Console]::Write('shell-ok')"
        } else {
            "printf shell-ok"
        };
        let output = run(parse_request(&serde_json::json!({ "command": command })).unwrap())
            .await
            .unwrap();
        assert_eq!(output.stdout, "shell-ok");
        assert_eq!(output.exit_code, Some(0));
        assert!(!output.timed_out);
    }

    #[tokio::test]
    async fn times_out_and_marks_the_result() {
        let command = if cfg!(windows) {
            "Start-Sleep -Milliseconds 500"
        } else {
            "sleep 0.5"
        };
        let output = run(parse_request(
            &serde_json::json!({ "command": command, "timeoutMs": 20 }),
        )
        .unwrap())
        .await
        .unwrap();
        assert!(output.timed_out);
    }
}

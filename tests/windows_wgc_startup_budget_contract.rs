use std::time::Duration;

const SHARE_WINDOWS: &str = include_str!("../src/computer/share_windows.rs");
const COMPUTER_HELPER: &str = include_str!("../src/bin/local-computer-helper.rs");

fn duration_seconds(source: &str, constant: &str) -> u64 {
    let prefix = format!("const {constant}: Duration = Duration::from_secs(");
    let line = source
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with(&prefix))
        .unwrap_or_else(|| panic!("missing duration constant {constant}"));
    line.strip_prefix(&prefix)
        .and_then(|value| value.strip_suffix(");"))
        .unwrap_or_else(|| panic!("invalid duration constant {constant}"))
        .parse()
        .unwrap_or_else(|_| panic!("non-numeric duration constant {constant}"))
}

#[test]
fn windows_wgc_startup_budget_precedes_the_helper_watchdog() {
    let startup = Duration::from_secs(duration_seconds(SHARE_WINDOWS, "STARTUP_TOTAL_TIMEOUT"));
    let watchdog = Duration::from_secs(duration_seconds(
        COMPUTER_HELPER,
        "COMMAND_WATCHDOG_TIMEOUT",
    ));

    assert_eq!(watchdog, Duration::from_secs(12));
    assert!(startup < watchdog);
    assert!(watchdog - startup >= Duration::from_secs(2));
    assert!(
        SHARE_WINDOWS.contains("const STARTUP_ROLLBACK_RESERVE: Duration = OWNER_STOP_TIMEOUT;")
    );
}

#[test]
fn windows_wgc_startup_rollback_uses_the_internal_deadline() {
    let startup = SHARE_WINDOWS
        .split("pub(crate) fn start")
        .nth(1)
        .and_then(|source| source.split("pub(crate) fn stop").next())
        .expect("NativeShareCapture::start source");
    assert!(startup.contains("startup_deadline"));
    assert!(startup.contains("readiness_deadline"));
    assert!(startup.contains("stop_startup_capture("));
    assert!(
        startup
            .contains("capture.confirm_reported_startup_failure_before(startup_deadline, &error)?")
    );
    assert!(!startup.contains("capture.stop()"));
    assert!(!startup.contains("exact_window_pair("));

    let descriptor_validation = SHARE_WINDOWS
        .split("fn validate_descriptor_geometry")
        .nth(1)
        .and_then(|source| source.split("fn validate_target_binding_geometry").next())
        .expect("pure descriptor validation source");
    assert!(!descriptor_validation.contains("exact_window_"));

    let owner_startup = SHARE_WINDOWS
        .split("fn create_capture_runtime")
        .nth(1)
        .and_then(|source| source.split("fn run_frame_callback").next())
        .expect("WGC owner startup source");
    assert!(owner_startup.contains("validate_target_binding_geometry(&target)"));
    assert!(owner_startup.contains("runtime.session.StartCapture()"));
    assert!(owner_startup.contains("exact_window_pair(&target)"));

    let rollback = SHARE_WINDOWS
        .split("fn stop_startup_capture")
        .nth(1)
        .and_then(|source| source.split("impl Drop for NativeShareCapture").next())
        .expect("startup rollback source");
    assert!(rollback.contains("capture.stop_before(deadline)"));
    assert!(rollback.contains("wait_for_owner_exit"));
    assert!(rollback.contains("STARTUP_JOIN_POLL_INTERVAL"));
    assert!(SHARE_WINDOWS.contains("self.stop_before(Instant::now() + OWNER_STOP_TIMEOUT)"));

    let reported_failure = SHARE_WINDOWS
        .split("fn confirm_reported_startup_failure_before")
        .nth(1)
        .and_then(|source| source.split("pub(crate) const fn metadata").next())
        .expect("reported startup failure confirmation source");
    assert!(reported_failure.contains("wait_for_owner_exit(&owner, deadline)"));
    assert!(reported_failure.contains("owner_error.code == reported_error.code"));
    assert!(reported_failure.contains("owner_error.message == reported_error.message"));
    assert!(reported_failure.contains("self.state.clear_latest()"));
}

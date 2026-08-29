use std::collections::BTreeSet;
use std::fs;
use std::process::Command;

use local_browser_bridge::VERSION;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use local_browser_bridge::computer::ComputerController;
use local_browser_bridge::computer::{
    COMPUTER_METHODS, COMPUTER_TYPE_TEXT_MAX_DISPATCH_MS, COMPUTER_TYPE_TEXT_MAX_UTF16_UNITS,
};
use sha2::{Digest as _, Sha256};

fn file_sha256(path: &str) -> String {
    format!("{:x}", Sha256::digest(fs::read(path).unwrap()))
}

fn walk_files(root: &std::path::Path) -> BTreeSet<String> {
    let mut pending = vec![root.to_owned()];
    let mut files = BTreeSet::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory).unwrap().map(Result::unwrap) {
            let file_type = entry.file_type().unwrap();
            assert!(!file_type.is_symlink());
            if file_type.is_dir() {
                pending.push(entry.path());
                continue;
            }
            assert!(file_type.is_file());
            let relative = entry.path().strip_prefix(root).unwrap().to_owned();
            files.insert(
                relative
                    .components()
                    .map(|component| component.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/"),
            );
        }
    }
    files
}

#[test]
fn helper_exposes_only_bounded_observation_and_input_methods() {
    let methods = COMPUTER_METHODS.iter().copied().collect::<BTreeSet<_>>();
    assert_eq!(
        methods,
        BTreeSet::from([
            "computer.click",
            "computer.drag",
            "computer.key",
            "computer.invoke",
            "computer.move",
            "computer.observe",
            "computer.scroll",
            "computer.setValue",
            "computer.share.start",
            "computer.share.status",
            "computer.share.stop",
            "computer.status",
            "computer.typeText",
        ])
    );
    for dangerous_fragment in [
        "shell",
        "file",
        "process",
        "clipboard",
        "download",
        "execute",
    ] {
        assert!(
            methods
                .iter()
                .all(|method| !method.contains(dangerous_fragment))
        );
    }
}

#[test]
fn unsupported_host_surface_reexports_server_handshake_capabilities() {
    let unsupported = fs::read_to_string("src/computer_unsupported.rs").unwrap();
    for capability in [
        "COMPUTER_INPUT_DELIVERY_PROVENANCE_CAPABILITY",
        "COMPUTER_POINTER_ACTIVITY_MONITOR_CAPABILITY",
    ] {
        assert!(
            unsupported.contains(capability),
            "unsupported-host module must re-export {capability} for cross-platform server builds"
        );
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn helper_machine_contract_discloses_platform_activation_truthfully() {
    let hello = ComputerController::new().hello();
    let invariants = &hello["invariants"];
    let capabilities = hello["capabilities"].as_array().unwrap();

    assert!(capabilities.iter().any(|capability| {
        capability.as_str() == Some("computer.input-delivery-provenance.v1")
    }));
    assert!(
        capabilities.iter().any(|capability| {
            capability.as_str() == Some("computer.pointer-activity-monitor.v1")
        })
    );

    assert_eq!(invariants["globalHidInput"], false);
    assert_eq!(invariants["movesHardwareCursor"], false);
    assert_eq!(invariants["foregroundIdentityPreservedBeforeAfter"], true);
    assert_eq!(invariants["hardwareCursorPreservedBeforeAfter"], true);
    assert_eq!(invariants["hardwareCursorSampleAuthoritative"], false);
    assert_eq!(invariants["inputDeliveryProvenanceRequired"], true);
    assert_eq!(invariants["usesAxRaise"], false);
    assert_eq!(invariants["usesFrontProcessSwitch"], false);
    assert_eq!(invariants["switchesActiveSpace"], false);
    assert_eq!(invariants["zeroTransientInterruptionGuaranteed"], false);
    assert_eq!(invariants["exactWindowRequired"], true);
    assert_eq!(invariants["implicitForegroundFallback"], false);

    #[cfg(target_os = "macos")]
    {
        assert_eq!(invariants["activatesTargetApplication"], true);
        assert_eq!(
            invariants["targetActivationMode"],
            "may-use-transient-ax-frontmost-focus-lease"
        );
    }
    #[cfg(target_os = "windows")]
    {
        assert_eq!(invariants["activatesTargetApplication"], false);
        assert_eq!(
            invariants["targetActivationMode"],
            "no-explicit-target-activation-api"
        );
    }
}

#[test]
fn macos_focus_lease_is_disclosed_without_overclaiming_zero_interruption() {
    let controller = fs::read_to_string("src/computer.rs").unwrap();
    let macos = fs::read_to_string("src/computer/platform_macos.rs").unwrap();
    let readme = fs::read_to_string("README.md").unwrap();
    let security = fs::read_to_string("SECURITY.md").unwrap();
    let install = fs::read_to_string("docs/INSTALL.md").unwrap();
    let capabilities = fs::read_to_string("docs/CAPABILITIES.md").unwrap();
    let protocol = fs::read_to_string("docs/PROTOCOL.md").unwrap();

    assert!(controller.contains("\"activatesTargetApplication\": cfg!(target_os = \"macos\")"));
    assert!(controller.contains("may-use-transient-ax-frontmost-focus-lease"));
    assert!(macos.contains("FocusOperation::Defocus"));
    assert!(macos.contains("FocusOperation::Focus"));
    assert!(macos.contains("!self.user_frontmost\n                    && self.target_frontmost"));
    assert!(macos.contains(
        "let focus = activate_without_raise(target, &before, cancellation, preparation_deadline)?;"
    ));
    assert!(!macos.contains("stage != InvariantStage::PointerTrajectory"));
    let move_path = macos
        .split("pub fn move_pointer_path(")
        .nth(1)
        .unwrap()
        .split("pub fn click(")
        .next()
        .unwrap();
    assert!(move_path.contains("|focus, delivery|"));
    assert!(
        move_path.contains("prove_action_dispatch_owner(focus, target, move_deadline, true)?;")
    );
    assert!(move_path.contains("post_before_deadline("));
    assert!(!macos.contains("fn post_mouse("));
    assert!(!macos.contains("AXRaise"));
    assert!(!macos.contains("_SLPSSetFrontProcessWithOptions("));

    assert!(readme.contains("briefly borrow and restore app focus"));
    assert!(readme.contains("[Limitations](docs/LIMITATIONS.md)"));
    for document in [&security, &install, &capabilities, &protocol] {
        assert!(document.contains("AXFrontmost"));
    }
    assert!(security.contains("activatesTargetApplication: true"));
    assert!(install.contains("not proof of zero visible or focus-state interruption"));
    assert!(protocol.contains("zeroTransientInterruptionGuaranteed"));
    assert!(!install.contains("activate the target app"));
    assert!(!security.contains("activate the target application, change"));
}

#[test]
fn helper_binary_reports_the_aligned_version_without_starting_a_daemon() {
    let output = Command::new(env!("CARGO_BIN_EXE_local-computer-helper"))
        .arg("--version")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        format!("local-computer-helper {VERSION}")
    );
}

#[test]
fn distributed_binaries_expose_project_and_locked_dependency_licenses() {
    for executable in [
        env!("CARGO_BIN_EXE_local-browser-bridge"),
        env!("CARGO_BIN_EXE_local-computer-helper"),
    ] {
        let output = Command::new(executable).arg("--licenses").output().unwrap();
        assert!(output.status.success());
        let report = String::from_utf8(output.stdout).unwrap();
        assert!(report.contains("MIT License"));
        assert!(report.contains("Local Browser Bridge third-party licenses"));
        assert!(report.contains("Apache License"));
        assert!(!report.contains("option-ext"));
        assert!(!report.contains("Mozilla Public License"));
        assert!(!report.contains("/Users/"));
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
#[test]
fn unsupported_host_helper_fails_before_connecting() {
    let output = Command::new(env!("CARGO_BIN_EXE_local-computer-helper"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .contains("Native computer control is available only on macOS and Windows")
    );
}

#[test]
fn helper_source_has_no_shell_filesystem_or_clipboard_implementation() {
    let helper = fs::read_to_string("src/bin/local-computer-helper.rs").unwrap();
    let controller = fs::read_to_string("src/computer.rs").unwrap();
    let macos = fs::read_to_string("src/computer/platform_macos.rs").unwrap();
    let windows = fs::read_to_string("src/computer/platform_windows.rs").unwrap();
    let source = format!("{helper}\n{controller}\n{macos}\n{windows}");
    for forbidden in [
        "std::process::Command",
        "tokio::process",
        "std::fs::",
        "tokio::fs::",
        "arboard",
        "clipboard_rs",
        "computer.shell",
    ] {
        assert!(
            !source.contains(forbidden),
            "found forbidden source: {forbidden}"
        );
    }
}

#[test]
fn background_backend_has_no_global_or_foreground_input_fallback() {
    let manifest = fs::read_to_string("Cargo.toml").unwrap();
    let macos = fs::read_to_string("src/computer/platform_macos.rs")
        .unwrap()
        .replace("\r\n", "\n");
    let windows = fs::read_to_string("src/computer/platform_windows.rs")
        .unwrap()
        .replace("\r\n", "\n");
    assert!(!manifest.contains("enigo"));
    for forbidden in [
        "CGEventTapLocation::HID",
        "CGEventPost(",
        "CGWarpMouseCursorPosition",
        "CGDisplayMoveCursorToPoint",
        "CGAssociateMouseAndMouseCursorPosition",
        "CGDisplayHideCursor",
        "CGDisplayShowCursor",
        "CGEventTapPostEvent",
        "CGPostMouseEvent",
        "CGPostScrollWheelEvent",
        "CGPostKeyboardEvent",
        "IOHIDPostEvent",
        "IOHIDSetCursorEnable",
        "IOHIDSetCursorPosition",
        "CGEventPostToPSN",
        "CGEventPostToPid",
        "SendInput(",
        "SetCursorPos(",
        "SetPhysicalCursorPos(",
        "SetForegroundWindow(",
        "AttachThreadInput(",
        "AllowSetForegroundWindow(",
        "LockSetForegroundWindow(",
        "SwitchToThisWindow(",
        "keybd_event(",
    ] {
        assert!(
            !macos.contains(forbidden) && !windows.contains(forbidden),
            "found forbidden foreground fallback: {forbidden}"
        );
    }
    assert!(!windows.contains("mouse_event("));
    assert!(macos.contains("SLEventPostToPid"));
    assert!(macos.contains("CGEventSetWindowLocation"));
    assert!(macos.contains("CGEventSource::new(CGEventSourceStateID::Private)"));
    assert!(macos.contains("CGEvent::new_mouse_event(\n        private_event_source()?"));
    assert!(macos.contains("CGEvent::new_keyboard_event(private_event_source()?"));
    assert!(macos.contains(
        "let source = private_event_source()?;\n            let event = CGEvent::new_scroll_event("
    ));
    assert!(windows.contains("PostMessageW"));
    assert!(windows.contains("RegisterRawInputDevices"));
    assert!(windows.contains("SetWindowsHookExW"));
    assert!(windows.contains("RIDEV_REMOVE"));
    assert!(windows.contains("RIDEV_INPUTSINK"));
    assert!(windows.contains("LLMHF_INJECTED"));
    assert!(windows.contains("sequence_before == sequence_after"));
    assert!(windows.contains("sequence_after & 1 == 0"));
    assert!(windows.contains("RIDEV_REMOVE requires a null target"));
    assert!(windows.contains("cleanup_acknowledged"));
    assert!(!windows.contains("SetWindowLongPtrW"));
    assert!(!windows.contains("WS_EX_NOACTIVATE"));
    assert!(macos.contains("held_event_sequence"));
    assert!(macos.contains("guarded_semantic"));
    assert!(macos.contains("restore_previous_after_failed_activation"));
    assert!(windows.contains("held_message_sequence"));
    assert!(windows.contains("guarded_effect"));
    assert!(windows.contains("release_keys"));
    assert!(windows.contains("OpenInputDesktop"));
    assert!(windows.contains("GetUserObjectInformationW"));
    assert!(windows.contains("ProcessIdToSessionId"));
    assert!(windows.contains("matches!(process_session_id, Some(1..))"));
    assert!(windows.contains("DesktopSnapshot::capture().is_ok()"));
    assert!(
        windows
            .contains("pub fn input_ready() -> bool {\n    interactive_input_desktop_ready()\n}")
    );
    assert!(windows.contains(
        "pub fn semantic_ready(_prompt: bool) -> bool {\n    interactive_input_desktop_ready()\n}"
    ));
    assert!(windows.contains("self.input_desktop == after.input_desktop"));
    assert!(!windows.contains("space_unchanged: true"));
    assert!(windows.contains("GetForegroundWindow returned no readable window"));
    assert!(windows.contains("GetGUIThreadInfo failed for the foreground thread"));
    assert!(macos.contains("ax_macos::application_focus_state_before(front_pid, focus_deadline)"));
    assert!(macos.contains("front_focus_before != front_focus_after"));
    assert!(macos.contains("!front_focus_after.frontmost"));
    assert!(macos.contains(
        "struct DesktopSnapshot {\n    front_process: [u8; 8],\n    front_pid: u32,\n    front_window_id: u32,"
    ));
    assert!(macos.contains("&lease.previous_psn"));
    assert!(macos.contains("lease.previous_window_id"));
}

#[test]
fn browser_and_native_key_grammars_are_documented_as_distinct_subsets() {
    let protocol = fs::read_to_string("docs/PROTOCOL.md").unwrap();
    let capabilities = fs::read_to_string("docs/CAPABILITIES.md").unwrap();
    let macos = fs::read_to_string("src/computer/platform_macos.rs").unwrap();
    let windows = fs::read_to_string("src/computer/platform_windows.rs").unwrap();

    assert!(
        protocol.contains("`page.key` and `computer.key` share only the server-side chord syntax")
    );
    assert!(protocol.contains("`computer.key` is narrower and platform-specific"));
    assert!(protocol.contains("`ContextMenu`, `CapsLock`, `PrintScreen`, and `Pause`"));
    assert!(protocol.contains("neither a control nor whitespace character"));
    assert!(protocol.contains("Literal `+` is reserved as the chord separator"));
    assert!(protocol.contains("exactly one UTF-16 code unit"));
    assert!(protocol.contains("Windows-key chord"));
    assert!(protocol.contains("COMPUTER_BACKGROUND_UNAVAILABLE"));
    assert!(capabilities.contains("| Native key subset |"));
    for key in ["\"f1\" => 122", "\"f12\" => 111", "\"pageup\" => 116"] {
        assert!(
            macos.contains(key),
            "missing documented macOS key map: {key}"
        );
    }
    for key in [
        "value if value.starts_with('f')",
        "\"pageup\" => 0x21",
        "\"`\" => 0xC0",
    ] {
        assert!(
            windows.contains(key),
            "missing documented Windows key map: {key}"
        );
    }
    assert!(windows.contains("let windows = chord.modifiers.contains(&VK_LWIN)"));
    assert!(windows.contains("let global_switch = alt && matches!(chord.key, VK_TAB | VK_ESCAPE)"));
    assert!(windows.contains("let secure_attention = control && alt && chord.key == VK_DELETE"));
}

#[test]
fn native_text_delivery_is_bounded_paced_and_cancellable() {
    assert_eq!(COMPUTER_TYPE_TEXT_MAX_UTF16_UNITS, 2_000);
    assert_eq!(COMPUTER_TYPE_TEXT_MAX_DISPATCH_MS, 2_500);

    let protocol = fs::read_to_string("src/computer_protocol.rs").unwrap();
    let controller = fs::read_to_string("src/computer.rs").unwrap();
    let server = fs::read_to_string("src/server.rs").unwrap();
    let macos = fs::read_to_string("src/computer/platform_macos.rs").unwrap();
    let windows = fs::read_to_string("src/computer/platform_windows.rs").unwrap();

    assert!(protocol.contains("validate_computer_type_text"));
    assert!(protocol.contains("text.encode_utf16().count()"));
    assert!(controller.contains("let utf16_units = validate_computer_type_text(text)?"));
    assert!(server.contains("validate_computer_type_text(text)"));

    assert!(macos.contains("const TEXT_EVENT_PACE: Duration = Duration::from_millis(1)"));
    assert!(macos.contains("pace_text_dispatch(cancellation, deadline, TEXT_EVENT_PACE)"));
    assert!(macos.contains("ensure_text_keyboard_receiver(focus, target, dispatched, deadline)?;"));
    assert!(windows.contains("const TEXT_MESSAGE_BURST: usize = 16"));
    assert!(windows.contains("ensure_unicode_keyboard_recipient(recipient)?"));
    assert!(windows.contains("WM_CHAR_REPEAT_COUNT"));
    assert!(windows.contains("pace_text_dispatch(cancellation, deadline, TEXT_MESSAGE_PACE)"));
    for source in [macos, windows] {
        assert!(source.contains("cancellation.check(\"native text dispatch\")"));
        assert!(source.contains("COMPUTER_OUTCOME_UNKNOWN"));
    }
}

#[test]
fn macos_keyboard_delivery_requires_exact_post_focus_receiver_proof() {
    let macos = fs::read_to_string("src/computer/platform_macos.rs")
        .unwrap()
        .replace("\r\n", "\n");
    let accessibility = fs::read_to_string("src/computer/ax_macos.rs")
        .unwrap()
        .replace("\r\n", "\n");

    assert!(macos.contains("copy_window_info(kCGWindowListOptionAll, kCGNullWindowID)"));
    assert!(macos.contains("ensure_keyboard_target_eligible(target)?;"));
    assert!(!macos.contains("ensure_unique_keyboard_destination"));
    assert!(!macos.contains("ensure_keyboard_dispatch_ready"));
    assert!(macos.contains("ax_macos::application_focus_state_before("));
    assert!(macos.contains("enum FocusLeasePhase"));
    assert!(macos.contains("ReleasedWithTargetPrior"));
    assert!(macos.contains("ReleasedWithTargetRequested"));
    assert!(macos.contains("ReleasedWithInactiveTargetRequested"));
    assert!(macos.contains("ReleasedWithActiveTargetPrior"));
    assert!(macos.contains("post_phase_transition_before"));
    assert!(macos.contains("FOCUS_USER_RECOVERY_RESERVE"));
    assert!(macos.contains("FOCUS_USER_AUTHORIZATION_RESERVE"));
    assert!(macos.contains("FOCUS_USER_RESTORE_POLL_BUDGET"));
    assert!(macos.contains("FOCUS_USER_RETRY_RESERVE"));
    assert!(macos.contains("prove_action_dispatch_owner("));
    assert!(macos.contains("focus.prove_dispatch_owner_before(deadline)"));
    assert!(macos.contains("FOCUS_PREPARATION_MIN_SETTLE"));
    assert!(macos.contains("self.focus_facts_before(classify_deadline, stage)"));
    assert!(macos.contains("restore_released_user_without_target_proof_before"));
    assert!(macos.contains("self.front_restore_destination_is_restorable_before(deadline)"));
    assert!(
        macos.contains("self.target_restore_destination_is_restorable_before(target_deadline)")
    );
    assert!(macos.contains("raw_keyboard_window_inventory_before(deadline)"));
    assert!(!macos.contains("raw_keyboard_window_inventory()"));
    assert!(macos.contains("raw_focus_window_restorable("));
    assert!(macos.contains("SLSGetWindowOwner"));
    assert!(macos.contains("SLSGetConnectionPSN"));
    assert!(macos.contains("post_make_key_window_record"));
    assert!(macos.contains("post_target_make_key_window_for_activation_before"));
    assert!(macos.contains("target exact receiver preparation"));
    assert!(macos.contains("record[0x3A] = 0x10"));
    assert!(macos.contains("record[0x20..0x30].fill(0xFF)"));
    assert!(macos.contains("record[0x08] = 0x01"));
    assert!(macos.contains("record[0x08] = 0x02"));
    assert!(!macos.contains("load(b\"_SLPSSetFrontProcessWithOptions"));
    assert!(!macos.contains("AXRaise"));
    assert!(accessibility.contains("pub struct ExactTargetMainWindow"));
    assert!(accessibility.contains("attribute_settable(window.as_ptr(), \"AXMain\")"));
    assert!(accessibility.contains("main_window_id"));
    assert!(macos.contains("ensure_same_process_mutation_target(target, before)?;"));
    assert!(macos.contains("previous_focus.window_id != before.front_window_id"));
    let semantic_guard = macos
        .split("fn guarded_semantic(")
        .nth(1)
        .unwrap()
        .split("fn mouse_event(")
        .next()
        .unwrap();
    assert!(!semantic_guard.contains("ensure_same_process_mutation_target"));
    assert!(macos.contains("ax_macos::accessibility_ready(false)"));
    assert!(macos.contains("FOCUS_RESTORE_PROOF_BUDGET"));

    let recovery_classifier = macos
        .split("fn classify_recovery(")
        .nth(1)
        .unwrap()
        .split("impl FocusLease")
        .next()
        .unwrap();
    assert!(
        recovery_classifier
            .find("if !target_may_be_prepared")
            .unwrap()
            < recovery_classifier.find("if requested_active").unwrap()
    );
    assert!(recovery_classifier.contains("FocusLeasePhase::Unknown"));

    let action_owner = macos
        .split("fn prove_action_dispatch_owner(")
        .nth(1)
        .unwrap()
        .split("fn prove_target_focus_window(")
        .next()
        .unwrap();
    assert!(action_owner.contains("&& Instant::now() < deadline"));
    assert_eq!(
        action_owner
            .matches("focus.prove_dispatch_owner_before(deadline)?;")
            .count(),
        2
    );

    let same_process_guard = macos
        .split("fn ensure_same_process_mutation_target(")
        .nth(1)
        .unwrap()
        .split("fn focus_preparation_needed(")
        .next()
        .unwrap();
    assert!(same_process_guard.contains("before.front_window_id"));
    assert!(same_process_guard.contains("front_process_identity(symbols).ok()"));
    assert!(same_process_guard.contains("snapshot_front_window_id == target_window_id"));

    let target_restore = macos
        .split("fn post_target_focus_record_before(")
        .nth(1)
        .unwrap()
        .split("fn restore_user_focus_before(")
        .next()
        .unwrap();
    assert!(
        target_restore.find("target_process_is_current()").unwrap()
            < target_restore
                .find("target_window_is_restorable_before(window_id, deadline)")
                .unwrap()
    );
    assert!(
        target_restore
            .find("target_application_focus_state_before(self.target_pid, deadline)")
            .unwrap()
            < target_restore
                .find("user_owner_matches_before(false, deadline)")
                .unwrap()
    );
    let target_cleanup = macos
        .split("fn restore_target_focus_from_phase_before(")
        .nth(1)
        .unwrap()
        .split("fn post_target_focus_record_before(")
        .next()
        .unwrap();
    for phase in [
        "ReleasedWithTargetRequested",
        "ReleasedWithInactiveTargetRequested",
        "ReleasedWithActiveTargetPrior",
        "ReleasedWithTargetPrior",
    ] {
        assert!(target_cleanup.contains(phase));
    }
    for required in [
        "await_known_active_target_before",
        "post_target_make_key_window_before",
        "FocusOperation::Focus",
        "FocusOperation::Defocus",
    ] {
        assert!(target_cleanup.contains(required));
    }
    assert!(
        target_cleanup.find("FocusOperation::Defocus").unwrap()
            < target_cleanup
                .find("FocusLeasePhase::ReleasedWithInactiveTargetRequested")
                .unwrap()
    );
    let user_restore = macos
        .split("fn restore_user_focus_before(")
        .nth(1)
        .unwrap()
        .split("fn restore_released_user_without_target_proof_before(")
        .next()
        .unwrap();
    assert!(
        user_restore.find("prove_phase_before(").unwrap()
            < user_restore
                .find("front_restore_destination_is_restorable_before(user_deadline)")
                .unwrap()
    );
    assert!(
        user_restore
            .find("front_restore_destination_is_restorable_before(user_deadline)")
            .unwrap()
            < user_restore
                .find("user_owner_matches_before(false, user_deadline)")
                .unwrap()
    );
    assert!(
        user_restore
            .find("user_owner_matches_before(false, user_deadline)")
            .unwrap()
            < user_restore.find("post_focus_record(").unwrap()
    );

    let restore = macos
        .split("fn restore(self)")
        .nth(1)
        .unwrap()
        .split("fn restore_previous_after_failed_activation(")
        .next()
        .unwrap();
    let user_focus_post = restore.find("restore_user_focus_before(").unwrap();
    let user_restored = restore[user_focus_post..]
        .find("await_user_owner_before(true, user_restored_deadline)")
        .unwrap()
        + user_focus_post;
    let target_restored = restore[user_restored..]
        .find("await_phase_before(")
        .unwrap()
        + user_restored;
    assert!(user_focus_post < user_restored && user_restored < target_restored);
    assert!(restore.contains("checked_sub(FOCUS_USER_RECOVERY_RESERVE)"));
    assert!(restore.contains("checked_sub(FOCUS_USER_AUTHORIZATION_RESERVE)"));
    assert!(restore.contains("checked_sub(FOCUS_USER_RETRY_RESERVE)"));

    let user_only_restore = macos
        .split("fn restore_released_user_without_target_proof_before(")
        .nth(1)
        .unwrap()
        .split("fn front_restore_destination_is_restorable_before(")
        .next()
        .unwrap();
    assert!(
        user_only_restore
            .find("front_restore_destination_is_restorable_before(deadline)")
            .unwrap()
            < user_only_restore
                .find("user_owner_matches_before(false, deadline)")
                .unwrap()
    );
    assert!(
        user_only_restore.find("post_focus_record(").unwrap()
            < user_only_restore
                .find("await_user_owner_before(true, deadline)")
                .unwrap()
    );

    let phase_transition = macos
        .split("fn post_phase_transition_before(")
        .nth(1)
        .unwrap()
        .split("fn set_target_main_window_before(")
        .next()
        .unwrap();
    let authorization = phase_transition
        .split("let authorize = ||")
        .nth(1)
        .unwrap()
        .split("authorize()?;")
        .next()
        .unwrap();
    assert!(
        authorization.find("prove_phase_before(").unwrap()
            < authorization
                .find("user_owner_matches_before(expected_user_frontmost, deadline)")
                .unwrap()
    );
    assert_eq!(phase_transition.matches("authorize()?;").count(), 2);
    assert!(
        phase_transition.find("authorize()?;").unwrap()
            < phase_transition
                .find("cancellation.begin_side_effect(boundary)?;")
                .unwrap()
    );
    assert!(
        phase_transition
            .find("cancellation.begin_side_effect(boundary)?;")
            .unwrap()
            < phase_transition.rfind("authorize()?;").unwrap()
    );

    let phase_facts = macos
        .split("fn focus_facts_before(")
        .nth(1)
        .unwrap()
        .split("fn user_focus_state_before(")
        .next()
        .unwrap();
    assert!(
        phase_facts
            .find("raw_keyboard_window_inventory_before(deadline)")
            .unwrap()
            < phase_facts
                .find("let user_before = self.user_focus_state_before")
                .unwrap()
    );
    assert!(
        phase_facts
            .find("let target_process_current = self.target_process_is_current();")
            .unwrap()
            < phase_facts
                .rfind("front_process_identity(self.symbols)")
                .unwrap()
    );

    for required_ax_proof in [
        "AXFocusedWindow",
        "AXFocusedUIElement",
        "AXTopLevelUIElement",
        "AXWindow",
        "AXParent",
        "AXMinimized",
        "AXHidden",
        "AXFrontmost",
        "MAX_KEYBOARD_PARENT_DEPTH",
        "MAX_KEYBOARD_TOP_LEVEL_ELEMENTS",
        "AX_ELIGIBILITY_TIMEOUT_SECONDS",
        "AX_RECEIVER_TIMEOUT_SECONDS",
        "KEYBOARD_RECEIVER_PROOF_BUDGET",
        "ensure_keyboard_receiver_before",
        "application_focus_state_impl",
        "target_application_focus_state",
    ] {
        assert!(
            accessibility.contains(required_ax_proof),
            "missing macOS keyboard AX proof: {required_ax_proof}"
        );
    }

    let text_start = macos.find("pub fn type_text(").unwrap();
    let text_end = macos[text_start..]
        .find("fn text_dispatch_checkpoint(")
        .unwrap()
        + text_start;
    let text = &macos[text_start..text_end];
    let guarded = text.find("InvariantStage::TextDispatch").unwrap();
    let eligibility = text
        .find("ensure_keyboard_target_eligible_before(target, deadline)?;")
        .unwrap();
    let event_construction = text.find("let down = keyboard_event").unwrap();
    assert!(eligibility < guarded && guarded < event_construction);
    let postconstruction_proof = text[event_construction..]
        .find("ensure_text_keyboard_receiver(focus, target, dispatched, deadline)?;")
        .unwrap()
        + event_construction;
    let postproof_deadline = text[postconstruction_proof..]
        .find("text_dispatch_checkpoint(cancellation, deadline)?;")
        .unwrap()
        + postconstruction_proof;
    let first_post = text[event_construction..]
        .find("held_event_sequence(")
        .unwrap()
        + event_construction;
    assert!(
        event_construction < postconstruction_proof
            && postconstruction_proof < postproof_deadline
            && postproof_deadline < first_post
    );
    let key_start = macos.find("pub fn key(").unwrap();
    let key_end = macos[key_start..]
        .find("fn ensure_keyboard_target_eligible(")
        .unwrap()
        + key_start;
    let key = &macos[key_start..key_end];
    let key_construction = key.find("let down = keyboard_event").unwrap();
    let key_receiver = key
        .find("ax_macos::ensure_keyboard_receiver_before(")
        .unwrap();
    let key_owner = key.find("prove_action_dispatch_owner(").unwrap();
    let key_post = key.find("held_event_sequence(").unwrap();
    assert!(key_construction < key_owner && key_owner < key_receiver && key_receiver < key_post);
    assert!(key.contains("let receiver_deadline = Instant::now() + FOCUS_RESTORE_PROOF_BUDGET;"));
    assert!(key.contains("prove_action_dispatch_owner(focus, target, receiver_deadline, false)?;"));
    assert!(key.contains("target, false, true, receiver_deadline"));
    assert!(key.contains("receiver_deadline,\n                &down"));

    let held_sequence = macos
        .split("fn held_event_sequence<T>(")
        .nth(1)
        .unwrap()
        .split("fn stamp(")
        .next()
        .unwrap();
    assert!(held_sequence.contains("post_before_deadline("));
    assert!(held_sequence.contains("first_post_deadline"));
    let deadline_post = macos
        .split("fn post_before_deadline(")
        .nth(1)
        .unwrap()
        .split("fn post_release(")
        .next()
        .unwrap();
    assert_eq!(
        deadline_post
            .matches("ensure_dispatch_deadline(cancellation, boundary, deadline)?;")
            .count(),
        2
    );
    assert!(deadline_post.contains("cancellation.begin_side_effect(boundary)?;"));

    let focus_preparation = macos
        .split("fn activate_without_raise(")
        .nth(1)
        .unwrap()
        .split("fn ensure_same_process_mutation_target(")
        .next()
        .unwrap();
    assert_eq!(
        focus_preparation
            .matches("lease.post_phase_transition_before(")
            .count(),
        2
    );
    assert!(focus_preparation.contains("let user_release_deadline = std::cmp::min("));
    assert!(focus_preparation.contains("let target_focus_deadline = std::cmp::min("));
    assert!(focus_preparation.contains("FocusLeasePhase::Restored"));
    assert!(focus_preparation.contains("FocusLeasePhase::RestoredWithInactiveTargetRequested"));
    assert!(focus_preparation.contains("FocusLeasePhase::ReleasedWithInactiveTargetRequested"));
    assert!(focus_preparation.contains("FocusLeasePhase::ReleasedWithTargetRequested"));
    assert!(focus_preparation.contains("exact_target_main_window_before"));
    assert!(
        focus_preparation
            .contains("target_previous_focus.main_window_id != target_previous_focus.window_id")
    );
    assert!(focus_preparation.contains("lease.set_target_main_window_before("));
    assert!(
        focus_preparation
            .find("lease.set_target_main_window_before(")
            .unwrap()
            < focus_preparation
                .find("let user_release_deadline = std::cmp::min(")
                .unwrap()
    );
    assert!(
        focus_preparation
            .find("previous_focus.window_id != before.front_window_id")
            .unwrap()
            < focus_preparation
                .find("lease.post_phase_transition_before(")
                .unwrap()
    );

    let text_dispatch_proof = macos
        .split("fn ensure_text_keyboard_receiver(")
        .nth(1)
        .unwrap()
        .split("fn map_text_receiver_failure(")
        .next()
        .unwrap();
    assert!(
        text_dispatch_proof
            .find("prove_action_dispatch_owner(focus, target, deadline, false)")
            .unwrap()
            < text_dispatch_proof
                .find("ensure_keyboard_receiver_before(target, true, true, deadline)")
                .unwrap()
    );

    for pointer_stage in ["ClickDispatch", "DragDispatch", "ScrollDispatch"] {
        let stage = macos
            .split(&format!("InvariantStage::{pointer_stage}"))
            .nth(1)
            .unwrap()
            .split("})")
            .next()
            .unwrap();
        assert!(stage.contains("prove_action_dispatch_owner("));
    }

    assert!(accessibility.contains(
        "enable_target_accessibility && enable_chromium_accessibility_once(pid, app.as_ptr())"
    ));
    assert!(!accessibility.contains("candidate_summaries"));
    assert!(!accessibility.contains("AXIdentifier="));
    assert!(!macos.contains("LBB_DIAG"));
}

#[test]
fn asynchronous_share_failures_are_bound_to_the_producing_epoch() {
    let helper = fs::read_to_string("src/bin/local-computer-helper.rs").unwrap();
    let controller = fs::read_to_string("src/computer.rs").unwrap();
    let server = fs::read_to_string("src/server.rs").unwrap();
    let protocol = fs::read_to_string("docs/PROTOCOL.md").unwrap();

    assert!(controller.contains("pub fn active_share_id(&self) -> Option<&str>"));
    assert!(helper.contains("let producing_share_id = controller.active_share_id()"));
    assert!(helper.contains("\"shareId\": producing_share_id"));
    assert!(helper.contains("\"shareId\": expired_share.share_id"));
    assert!(server.contains("authorize_computer_share_error"));
    assert!(server.contains("COMPUTER_SHARE_SESSION_EXHAUSTED"));
    assert!(protocol.contains("An unbound or old-share error is ignored"));
}

#[test]
fn unsupported_host_share_identity_surface_fails_closed_with_api_parity() {
    let unsupported = fs::read_to_string("src/computer_unsupported.rs").unwrap();
    assert!(unsupported.contains("pub fn active_share_id(&self) -> Option<&str>"));
    assert!(unsupported.contains("pub fn active_share_id(&self) -> Option<&str> {\n        None"));
    assert!(unsupported.contains("assert_eq!(controller.active_share_id(), None)"));
}

#[test]
fn background_invariant_failures_use_stage_bound_closed_vocabulary() {
    let controller = fs::read_to_string("src/computer.rs").unwrap();
    let production_controller = controller
        .split("#[cfg(test)]\nmod tests")
        .next()
        .expect("computer production source must precede its test module");
    let macos = fs::read_to_string("src/computer/platform_macos.rs").unwrap();
    let windows = fs::read_to_string("src/computer/platform_windows.rs").unwrap();
    let source = format!("{production_controller}\n{macos}\n{windows}");

    for stage in [
        "pointerTrajectory",
        "clickDispatch",
        "dragDispatch",
        "scrollDispatch",
        "textDispatch",
        "keyDispatch",
        "semanticInvoke",
        "semanticSetValue",
        "focusPreparation",
        "focusRestore",
        "focusRecovery",
    ] {
        assert!(
            production_controller.contains(stage),
            "missing invariant stage {stage}"
        );
    }
    for invariant in [
        "foregroundUnchanged",
        "userFocusUnchanged",
        "hardwareCursorPreservedByHelper",
        "sharedPointerBoundaryCorroborated",
        "inputRouteTargetBound",
        "desktopSpaceUnchanged",
    ] {
        assert!(
            production_controller.contains(invariant),
            "missing invariant name {invariant}"
        );
    }

    assert!(production_controller.contains("stage={};failedInvariants={}"));
    assert!(!source.contains(".assert_held()"));
    assert!(source.contains("InvariantStage::PointerTrajectory"));
    assert!(source.contains("InvariantStage::ClickDispatch"));
    assert!(source.contains("InvariantStage::DragDispatch"));
    assert!(source.contains("InvariantStage::ScrollDispatch"));
    assert!(!production_controller.contains(
        "The foreground, hardware cursor, or active desktop changed during background delivery"
    ));
}

#[test]
fn macos_v0_12_27_pointer_evidence_is_bounded_corroboration_not_causal_attribution() {
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs").unwrap();
    let probe = fs::read_to_string("evidence/v0.12.59/computer/SystemProbe.swift").unwrap();
    assert!(rig.contains("failureProbeBaseline"));
    assert!(rig.contains("collectFailureDiagnostics"));
    assert!(rig.contains("systemInvariants(failureProbeBaseline.system, after)"));
    assert!(rig.contains("fixtureCounterSnapshot"));
    assert!(rig.contains("semanticValueMatchesExpected"));
    assert!(rig.contains("failureDiagnostics,"));
    for required in [
        "schemaVersion: 9",
        "computer.input-delivery-provenance.v1",
        "computer.pointer-activity-monitor.v1",
        "inputDeliveryProvenanceV1",
        "pointerActivityMonitorV1",
        "function inputDeliveryProvenanceHeld(invariants)",
        "hardwareCursorPreservedByHelper",
        "helperGlobalPointerPreservation === \"confirmed\"",
        "sharedPointerBoundaryCorroborated",
        "sharedPointerBoundaryState === \"corroborated\"",
        "sharedPointerActivityObserved",
        "hidSystemPointerActivityObserved",
        "rawInputPointerActivityObserved",
        "injectedPointerActivityObserved",
        "pointerActivityMonitorHealthy",
        "sharedPointerActivityState",
        "const POINTER_EVIDENCE_LANES = new Set([\"quiet\", \"deliberate-concurrency\"])",
        "independentPointerLaneState(appShareActionInvariants) === \"quiet\"",
        "independentPointerLaneState(actionTransitionInvariants) === \"quiet\"",
        "function independentPointerLaneState(invariants)",
        "function actionPointerLaneState(invariants)",
        "function independentPointerLaneAccepted(invariants)",
        "function actionPointerLaneAccepted(invariants)",
        "pointerHandoffProductBoundaryQuiet = actionPointerLaneState(postResizeClick.invariants) === \"quiet\"",
        "action and independent pointer classifier separation self-test failed",
        "pointerHandoffProductBoundaryQuiet =",
        "pointerHandoffIndependentBoundaryQuiet =",
        "sharedHidInputObserved: pointerHandoffSharedHidInputObserved",
        "physicalHumanProvenanceClaimed: false",
        "cryptographicToolIdentityClaimed: false",
        "rawCursorPositionsRetained: false",
        "rawPlatformActivityCountersRetained: false",
        "hidSystemActivityClaimedAsPhysical: false",
        "assertNoRetainedPointerRawData(serialized",
        "assertNoRetainedPointerRawData(persistedLog",
    ] {
        assert!(
            rig.contains(required),
            "missing v0.12.59 pointer contract: {required}"
        );
    }
    assert!(!rig.contains("cursorUnchanged"));
    for required in [
        "CGEventSource.counterForEventType(.hidSystemState",
        "hidPointerCounters",
        "pointerBoundaryActivityObserved",
        "pointerActivityMonitorHealthy",
        "Only bounded equality/activity/health booleans and state enums are retained",
    ] {
        assert!(
            probe.contains(required),
            "missing ephemeral macOS pointer probe: {required}"
        );
    }

    let diagnostics = rig
        .split("async function collectFailureDiagnostics()")
        .nth(1)
        .unwrap()
        .split("function requireActionInvariants")
        .next()
        .unwrap();
    for raw_identity in [
        "foregroundPID:",
        "frontWindowID:",
        "cursorX:",
        "cursorY:",
        "hidPointerCounters:",
        "activeSpace:",
        "pid:",
        "targetFocusedWindowID:",
        "primaryWindowId:",
        "siblingWindowId:",
        "fixtureTargetPid:",
    ] {
        assert!(
            !diagnostics.contains(raw_identity),
            "failure evidence persisted raw identity {raw_identity}"
        );
    }
}

#[test]
fn macos_v0_12_27_action_pointer_classifier_matches_the_sealed_action_schema() {
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let server = fs::read_to_string("src/server.rs")
        .unwrap()
        .replace("\r\n", "\n");

    let independent_classifier = rig
        .split("function independentPointerLaneState(invariants)")
        .nth(1)
        .unwrap()
        .split("function actionPointerLaneState(invariants)")
        .next()
        .unwrap();
    for required in [
        "hidSystemKeyboardActivityObserved",
        "keyboardActivityMonitorHealthy",
        "sharedKeyboardBoundaryCorroborated",
        "sharedKeyboardBoundaryState",
        "sharedKeyboardActivityState",
        "sharedInputSeatActivityObserved",
    ] {
        assert!(
            independent_classifier.contains(required),
            "independent system classifier stopped requiring {required}"
        );
    }

    let action_classifier = rig
        .split("function actionPointerLaneState(invariants)")
        .nth(1)
        .unwrap()
        .split("function recordPointerLaneState(state)")
        .next()
        .unwrap();
    for required in [
        "cursorPositionUnchanged",
        "sharedPointerActivityObserved",
        "hidSystemPointerActivityObserved",
        "rawInputPointerActivityObserved",
        "injectedPointerActivityObserved",
        "pointerActivityMonitorHealthy",
        "sharedPointerBoundaryCorroborated",
        "sharedPointerBoundaryState",
        "sharedPointerActivityState",
    ] {
        assert!(
            action_classifier.contains(required),
            "action pointer classifier is missing sealed field {required}"
        );
    }
    for forbidden in [
        "hidSystemKeyboardActivityObserved",
        "keyboardActivityMonitorHealthy",
        "sharedKeyboardBoundaryCorroborated",
        "sharedKeyboardBoundaryState",
        "sharedKeyboardActivityState",
        "sharedInputSeatActivityObserved",
    ] {
        assert!(
            !action_classifier.contains(forbidden),
            "action pointer classifier incorrectly requires independent-only field {forbidden}"
        );
    }

    let independent_gate = rig
        .split("function allIndependentInvariantsHeld(invariants)")
        .nth(1)
        .unwrap()
        .split("function allActionInvariantsHeld(invariants)")
        .next()
        .unwrap();
    assert!(independent_gate.contains("independentPointerLaneAccepted(invariants)"));
    assert!(!independent_gate.contains("actionPointerLaneAccepted(invariants)"));

    let action_gate = rig
        .split("function allActionInvariantsHeld(invariants)")
        .nth(1)
        .unwrap()
        .split("function pointerEvidenceSummary()")
        .next()
        .unwrap();
    assert!(action_gate.contains("actionPointerLaneAccepted(invariants)"));
    assert!(!action_gate.contains("independentPointerLaneAccepted(invariants)"));
    assert!(rig.contains(
        "pointerHandoffProductBoundaryQuiet = actionPointerLaneState(postResizeClick.invariants) === \"quiet\""
    ));

    let sealed_action_keys = server
        .split("fn valid_native_computer_action_result")
        .nth(1)
        .unwrap()
        .split("let boolean = |name: &str|")
        .next()
        .unwrap();
    for required in [
        "cursorPositionUnchanged",
        "sharedPointerActivityObserved",
        "hidSystemPointerActivityObserved",
        "rawInputPointerActivityObserved",
        "injectedPointerActivityObserved",
        "pointerActivityMonitorHealthy",
        "sharedPointerBoundaryCorroborated",
        "sharedPointerBoundaryState",
        "sharedPointerActivityState",
    ] {
        assert!(sealed_action_keys.contains(required));
    }
    for forbidden in [
        "hidSystemKeyboardActivityObserved",
        "keyboardActivityMonitorHealthy",
        "sharedKeyboardActivityState",
        "sharedInputSeatActivityObserved",
    ] {
        assert!(
            !sealed_action_keys.contains(forbidden),
            "sealed native action schema unexpectedly contains {forbidden}"
        );
    }
}

#[test]
fn macos_v0_12_14_quiet_lane_stabilizes_the_native_seat_before_candidate_execution() {
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let probe = fs::read_to_string("evidence/v0.12.59/computer/SystemProbe.swift")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "const QUIET_SEAT_REQUIRED_STABLE_MS = 30_000;",
        "const QUIET_SEAT_MAXIMUM_WAIT_MS = 30 * 60_000;",
        "const QUIET_SEAT_SAMPLE_INTERVAL_MS = 500;",
        "const QUIET_SEAT_REQUIRED_STABLE_TRANSITIONS = 60;",
        "async function runNativeQuietSeatStabilization({",
        "function quietSeatTransitionDisposition(before, after)",
        "pointerState === \"concurrent-shared-seat-activity\"",
        "invariants.foregroundUnchanged !== true",
        "invariants.userFocusUnchanged !== true",
        "invariants.spaceUnchanged !== true",
        "native quiet-seat monitoring became unknown or unhealthy before product execution",
        "summary.completedBeforeCandidateExecution = true",
        "rawPointerDataRetained: false",
        "macOS quiet-seat execution regressions passed: stable completion, complete oracle classification, fixed reset causes, bounded unknown/probe refusal, reset-to-unknown refusal, monotonic interval/deadline, integer persisted duration/timeouts, source-only readiness.",
    ] {
        assert!(rig.contains(required), "quiet-seat rig omits {required}");
    }
    for required in [
        "foregroundProbeHealthy",
        "foregroundTransitionObserved",
        "foregroundAXProbeHealthy",
        "activeSpaceProbeHealthy",
        "pointerActivityMonitorHealthy",
    ] {
        assert!(probe.contains(required), "SystemProbe omits {required}");
    }

    let probe_compile = rig
        .find("run(\"xcrun\", [\"swiftc\", systemProbeSource")
        .unwrap();
    let permission_probe = rig
        .find("const permissionProbe = processProbe(systemProbeBinary);")
        .unwrap();
    let gate = rig
        .rfind("const stabilized = await runNativeQuietSeatStabilization({")
        .unwrap();
    let lane_start = rig
        .find("laneStartedAt = new Date().toISOString();")
        .unwrap();
    let server_version = rig
        .find("exactVersion(serverPath, \"local-browser-bridge\")")
        .unwrap();
    let helper_version = rig
        .find("exactVersion(helperPath, \"local-computer-helper\")")
        .unwrap();
    let fixture_spawn = rig.find("fixtureProcess = spawn(fixtureBinary").unwrap();
    let server_spawn = rig.find("serverProcess = spawn(serverPath").unwrap();
    let helper_start = rig.find("startHelperOnce(helperEnvironment);").unwrap();
    assert!(
        probe_compile < permission_probe
            && permission_probe < gate
            && gate < lane_start
            && lane_start < server_version
            && server_version < helper_version
            && helper_version < fixture_spawn
            && fixture_spawn < server_spawn
            && server_spawn < helper_start,
        "candidate, fixture, server, or helper execution can precede native quiet-seat stabilization"
    );

    let whole_run = rig
        .find("requireIndependentInvariants(\"whole-run\", independentInvariants);")
        .unwrap();
    assert!(whole_run > helper_start);
    assert!(rig.contains("requireActionInvariants"));
    assert!(rig.contains("requireIndependentInvariants"));

    let stabilization = rig
        .split("async function runNativeQuietSeatStabilization({")
        .nth(1)
        .unwrap()
        .split("function preDispatchPointerTransitionDisposition")
        .next()
        .unwrap();
    assert!(stabilization.contains("monotonicMilliseconds = () => performance.now()"));
    assert!(!stabilization.contains("Date.now"));
    let live_gate = rig
        .split("const permissionProbe = processProbe(systemProbeBinary);")
        .nth(1)
        .unwrap()
        .split("laneStartedAt = new Date().toISOString();")
        .next()
        .unwrap();
    assert!(
        live_gate.contains("const quietSeatStartedAtMonotonicMilliseconds = performance.now();")
    );
    assert!(live_gate.contains("monotonicMilliseconds: () => performance.now(),"));
    assert!(!live_gate.contains("Date.now"));
}

#[test]
fn macos_v0_12_38_quiet_readiness_is_source_only_sanitized_and_non_evidence() {
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let finalizer = fs::read_to_string("scripts/finalize-macos-acceptance.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let probe = fs::read_to_string("evidence/v0.12.59/computer/SystemProbe.swift")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "rigArguments.length === 1 && rigArguments[0] === \"--quiet-readiness\"",
        "quietReadinessNonzeroSelfTestMode",
        "--quiet-readiness-self-test-nonzero",
        "process.exit(await runQuietReadinessMode({",
        "forceProbeNonzero: quietReadinessNonzeroSelfTestMode",
        "const QUIET_SEAT_DIAGNOSTIC_SCHEMA_VERSION = 1;",
        "const QUIET_SEAT_RESET_CAUSES = [",
        "const QUIET_SEAT_UNKNOWN_CAUSES = [",
        "const QUIET_SEAT_PROBE_FAILURE_CATEGORIES = [",
        "function quietSeatTransitionDisposition(before, after)",
        "return { kind: \"reset\", cause: \"foreground-changed\" };",
        "return { kind: \"reset\", cause: \"focus-changed\" };",
        "return { kind: \"reset\", cause: \"active-space-changed\" };",
        "function quietSeatReadinessRecord(status, summary)",
        "kind: \"macos-quiet-seat-readiness\"",
        "acceptanceEvidence: false",
        "candidateInvocations: 0",
        "rawProbeDataRetained: false",
        "async function executeQuietSeatReadiness({",
        "monotonicMilliseconds = () => performance.now()",
        "async function runQuietReadinessMode({ forceProbeNonzero = false } = {})",
        "run(\"xcrun\", [\"swiftc\", sourcePath, \"-o\", binaryPath]);",
        "run(process.execPath, [\"-e\", \"process.exit(7)\"]);",
        "processProbe(binaryPath, null, null, timeoutMilliseconds)",
        "record.status === \"ready\" ? 0 : 1",
        "lastResetCause",
        "lastUnknownCause",
        "resetCauseCounts",
        "probeFailureCategory",
        "foreground-ax-probe-unhealthy",
        "pointer-boundary-uncorroborated",
        "SUBPROCESS_TIMEOUT",
        "SUBPROCESS_NONZERO_EXIT",
        "SUBPROCESS_MALFORMED_OUTPUT",
        "source-only quiet readiness did not emit a sanitized non-evidence ready record",
        "source-only quiet readiness did not preserve bounded reset-to-unknown refusal",
        "foreground transition masked unreadable oracle category",
        "a wall-clock jump changed the monotonic quiet-seat sample interval",
        "a wall-clock jump extended or shortened the immutable 30-minute monotonic deadline",
        "summary.stableDurationMilliseconds = Math.floor(stableDurationMilliseconds);",
        "const probeTimeoutMilliseconds = Math.floor(",
        "sample = await probe(probeTimeoutMilliseconds);",
        "fractional monotonic samples did not persist a floored integer duration and integer probe timeouts",
        "real quiet-readiness CLI did not classify a startup-order nonzero subprocess fail closed",
    ] {
        assert!(rig.contains(required), "quiet readiness omits {required}");
    }

    let record = rig
        .split("function quietSeatReadinessRecord(status, summary)")
        .nth(1)
        .unwrap()
        .split("function quietSeatProbeFailureSummary")
        .next()
        .unwrap();
    for forbidden in [
        "foregroundPID",
        "rawForegroundPSN",
        "frontWindowID",
        "cursorX",
        "cursorY",
        "hidPointerCounters",
        "hidKeyboardCounters",
        "errorMessage",
        "sourcePath",
        "binaryPath",
    ] {
        assert!(
            !record.contains(forbidden),
            "quiet readiness record retains raw probe detail {forbidden}"
        );
    }

    let mode = rig
        .split("async function runQuietReadinessMode({ forceProbeNonzero = false } = {})")
        .nth(1)
        .unwrap()
        .split("async function runNativeQuietSeatStabilization")
        .next()
        .unwrap();
    for candidate_reference in [
        "serverPath",
        "helperPath",
        "archivePath",
        "sumsPath",
        "releaseCandidateBinding",
        "exactVersion(",
    ] {
        assert!(
            !mode.contains(candidate_reference),
            "quiet readiness inspects candidate material through {candidate_reference}"
        );
    }

    let classifier = rig
        .split("function quietSeatSampleMonitoringState(sample)")
        .nth(1)
        .unwrap()
        .split("function quietSeatTransitionDisposition(before, after)")
        .next()
        .unwrap();
    for classifier_contract in [
        "typeof sample?.foregroundIdentityStable !== \"boolean\"",
        "typeof sample?.rawForegroundIdentityStable !== \"boolean\"",
        "const workspaceIdentityReadable =",
        "const rawIdentityReadable =",
        "const rawIdentityUnavailable =",
        "const foregroundAXIdentityReadable =",
        "const foregroundAXUnavailable =",
        "if (sample.foregroundIdentityStable === false)",
        "sample.rawForegroundIdentityStable !== false",
        "sample?.foregroundPID !== 0",
        "sample?.frontWindowID !== 0",
        "if (sample.rawForegroundIdentityStable === false)",
        "if (!workspaceIdentityReadable || !rawIdentityUnavailable)",
        "if (!foregroundAXUnavailable)",
        "if (!foregroundAXIdentityReadable)",
        "return { kind: \"unknown\", cause: \"transition-unclassified\" };",
    ] {
        assert!(
            classifier.contains(classifier_contract),
            "quiet transition classifier is missing {classifier_contract}"
        );
    }
    for source_contract in [
        "let foregroundTransitionObserved = foregroundProbeHealthy &&\n    (!foregroundIdentityStable || !rawForegroundIdentityStable)",
        "let foregroundAXProbeHealthy = foregroundIdentityStable &&",
        "let foregroundPID = foregroundIdentityStable ? foregroundPIDBefore : 0",
        "\"foregroundAXFocusedWindowID\": foregroundIdentityStable ? foregroundAXFocusedWindowID : 0",
        "\"foregroundAXMainWindowID\": foregroundIdentityStable ? foregroundAXMainWindowID : 0",
        "\"foregroundAXFrontmost\": foregroundIdentityStable && foregroundAXFrontmost",
        "\"frontWindowID\": frontWindowIdentifier(\n        for: foregroundPID,",
        "\"rawForegroundPID\": rawForegroundIdentityStable ? rawForegroundBefore!.pid : 0",
        "\"rawForegroundPSN\": rawForegroundIdentityStable\n        ? processSerialNumberHex(rawForegroundBefore!.processSerialNumber)\n        : \"\"",
    ] {
        assert!(
            probe.contains(source_contract),
            "SystemProbe transition emission changed without the JS classifier: {source_contract}"
        );
    }
    for fixture_contract in [
        "function rigSelfTestWorkspaceForegroundTransitionSample()",
        "function rigSelfTestRawForegroundTransitionSample()",
        "foregroundPID: 0,\n    foregroundTransitionObserved: true,\n    foregroundIdentityStable: false,\n    rawForegroundIdentityStable: false,",
        "foregroundIdentityStable: true,\n    rawForegroundIdentityStable: false,\n    rawForegroundPID: 0,\n    rawForegroundPSN: \"\",",
        "a real SystemProbe foreground transition shape was not classified as a reset",
        "a real SystemProbe foreground transition did not retain its bounded reset category",
    ] {
        assert!(
            rig.contains(fixture_contract),
            "quiet transition fixture is missing {fixture_contract}"
        );
    }
    assert!(!rig.contains("baseline: { ...readable, foregroundTransitionObserved: true }"));

    let delay_initialization = rig.find("const delay = (milliseconds)").unwrap();
    let standalone_dispatch = rig.find("if (selfTestMode) {").unwrap();
    assert!(
        delay_initialization < standalone_dispatch,
        "standalone CLI dispatch precedes module-scoped dependency initialization"
    );

    let quiet_validation = finalizer
        .split("function validateQuietSeatStabilization(value, lane, label)")
        .nth(1)
        .unwrap()
        .split("function validateAssertions")
        .next()
        .unwrap();
    assert!(quiet_validation.contains("exactInteger(\n    value.stableDurationMilliseconds,"));
    assert!(quiet_validation.contains("exactInteger(\n    value.observedSamples,"));
    assert!(quiet_validation.contains("exactInteger(\n    value.stableTransitions,"));
    assert!(quiet_validation.contains("value.resetCauseCounts[value.lastResetCause] < 1"));
}

#[test]
fn macos_v0_12_27_app_share_handoff_is_exact_non_authoritative_and_fail_closed() {
    let app = fs::read_to_string("evidence/v0.12.59/computer/AppShareHandoff.swift")
        .unwrap()
        .replace("\r\n", "\n");
    let probe = fs::read_to_string("evidence/v0.12.59/computer/SystemProbe.swift")
        .unwrap()
        .replace("\r\n", "\n");
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let readme = fs::read_to_string("evidence/v0.12.59/computer/README.md")
        .unwrap()
        .replace("\r\n", "\n");
    let normalized_readme = readme.split_whitespace().collect::<Vec<_>>().join(" ");

    for required in [
        "private let productVersion = \"0.12.59\"",
        "private let stableWindowTitle = \"LBB macOS Acceptance App Share\"",
        "private let readyButtonTitle = \"START APP-SHARE CHECK\"",
        "private let armWindowSeconds: TimeInterval = 300",
        "private let completionGraceSeconds: TimeInterval = 18",
        "private func validateHandoffInvocation(",
        "hardRemaining <= armWindowSeconds + completionGraceSeconds",
        "completionGrace <= completionGraceSeconds",
        "startedAtDate.timeIntervalSince(receiptCreatedAt) <= completionGraceSeconds",
        "receiptCreatedAt.addingTimeInterval(completionGraceSeconds)",
        "completedAtDate.timeIntervalSince(startedAtDate) <= completionGraceSeconds",
        "completedAtDate.timeIntervalSince(receiptCreatedAt) <= completionGraceSeconds",
        "acceptedHardExpiration.addingTimeInterval(0.001)",
        "300s arm and 18s completion accepted; beyond-policy invocation refused",
        "private final class NonactivatingHandoffPanel: NSPanel",
        "override var canBecomeKey: Bool { false }",
        "override var canBecomeMain: Bool { false }",
        "styleMask: [.titled, .nonactivatingPanel]",
        "panel.ignoresMouseEvents = false",
        "panel.acceptsMouseMovedEvents = false",
        "panel.hidesOnDeactivate = false",
        "actionButton.setAccessibilityIdentifier(\"lbb-app-share-start\")",
        "@objc private func receiveAppShareAction()",
        "guard state == .ready, actionButton.isEnabled, let requestSha256,",
        "secureSha256(path: requestPath) == requestSha256",
        "actionButton.isEnabled = false",
        "startReceiptSha256 = receiptSha256",
        "startReceiptSha256 == receiptSha",
        "productActionStartedAt == control.productActionStartedAt",
        "productActionCompletedAt == control.productActionCompletedAt",
        "\"buttonActionObserved\": true",
        "\"physicalHumanProvenanceClaimed\": false",
        "\"cryptographicToolIdentityClaimed\": false",
        "\"acceptedAsAuthority\": false",
        "\"kind\": \"macos-app-share-concurrency-handoff-start\"",
        "\"kind\": \"macos-app-share-concurrency-handoff-complete\"",
        "\"buttonRemainedDisabledDuringProductAction\": true",
        "\"handoffStateSequenceBound\": true",
        "\"requestSha256\": control.requestSha256",
        "\"startReceiptSha256\": receiptSha",
        "UInt32(RENAME_EXCL)",
        "macOS app-share handoff self-test passed",
    ] {
        assert!(
            app.contains(required),
            "exact app-share handoff app is missing: {required}"
        );
    }
    for required in [
        "open(path, O_RDONLY | O_NOFOLLOW | O_NONBLOCK)",
        "ownerPrivateOrdinaryFile(metadata, maximumBytes: 16 * 1024)",
        "lstat(path, &pathMetadata)",
        "lstat(path, &pathMetadataAfter)",
        "sameStableFileIdentity(metadata, metadataAfter)",
        "sameStableFileIdentity(metadataAfter, pathMetadataAfter)",
        "private func secureReadText(path: String, maximumBytes: Int64)",
        "case .missing, .changed:",
        "case .invalid:",
    ] {
        assert!(
            app.contains(required),
            "secure request binding omits {required}"
        );
    }
    for forbidden in [
        "NSApp.activate",
        "application.activate",
        "NSRunningApplication.activate",
        "AXUIElementPerformAction",
        "makeKeyAndOrderFront",
        "makeKey()",
        "performClick(",
        "CGEventTapCreate",
        "CGEvent.tapCreate",
        "CGEventCreateMouseEvent",
        "CGEventCreateKeyboardEvent",
        "CGEventPost",
        "CGPostMouseEvent",
        "CGWarpMouseCursorPosition",
        "CGAssociateMouseAndMouseCursorPosition",
        "IOHIDPostEvent",
        "IOHIDEventSystemClientDispatchEvent",
        ".post(tap:",
        "NSEvent.addGlobalMonitorForEvents",
        "NSEvent.addLocalMonitorForEvents",
    ] {
        assert!(
            !app.contains(forbidden),
            "acceptance app must not activate or synthesize shared-seat input: {forbidden}"
        );
    }

    let app_arm_seconds = app
        .lines()
        .find_map(|line| {
            line.strip_prefix("private let armWindowSeconds: TimeInterval = ")
                .and_then(|value| value.parse::<u64>().ok())
        })
        .unwrap();
    let app_completion_seconds = app
        .lines()
        .find_map(|line| {
            line.strip_prefix("private let completionGraceSeconds: TimeInterval = ")
                .and_then(|value| value.parse::<u64>().ok())
        })
        .unwrap();
    let rig_arm_milliseconds = rig
        .lines()
        .find_map(|line| {
            line.strip_prefix("const APP_SHARE_HANDOFF_WAIT_MS = ")
                .and_then(|value| value.strip_suffix(';'))
                .map(|value| value.replace('_', ""))
                .and_then(|value| value.parse::<u64>().ok())
        })
        .unwrap();
    let rig_completion_milliseconds = rig
        .lines()
        .find_map(|line| {
            line.strip_prefix("const APP_SHARE_HANDOFF_COMPLETION_GRACE_MS = ")
                .and_then(|value| value.strip_suffix(';'))
                .map(|value| value.replace('_', ""))
                .and_then(|value| value.parse::<u64>().ok())
        })
        .unwrap();
    assert_eq!(rig_arm_milliseconds, app_arm_seconds * 1_000);
    assert_eq!(rig_completion_milliseconds, app_completion_seconds * 1_000);
    assert_eq!((app_arm_seconds, app_completion_seconds), (300, 18));
    for stale_bound in [
        "hardRemaining <= 310",
        "completionGrace <= 10",
        "addingTimeInterval(10)",
        "timeIntervalSince(receiptCreatedAt) <= 10",
        "timeIntervalSince(startedAtDate) <= 10",
    ] {
        assert!(
            !app.contains(stale_bound),
            "app-share handoff retained the old 10s timing policy: {stale_bound}"
        );
    }

    for required in [
        "private enum AppSharePromptState: String",
        "case ready = \"READY\"",
        "\"LBB macOS Acceptance App Share\"",
        "case .ready: \"START APP-SHARE CHECK\"",
        "var buttonEnabled: Bool { self == .ready }",
        "private func exactAppShareButton(",
        "windowID: CGWindowID",
        "axWindowIdentifier(matchingWindows[0], &matchingWindowID) == .success",
        "axString(element, \"AXIdentifier\" as CFString) == \"lbb-app-share-start\"",
        "guard pending.isEmpty, matchingButtons.count == 1",
        "private func appSharePromptObservation(",
        "\"dev.flrngel.local-browser-bridge.acceptance.app-share\"",
        "exactWindows.count == 1 && ownedWindows.count == 1",
        "exactWindows[0][kCGWindowNumber as String]",
        "button.windowID == compositorWindowID",
        "foregroundPID != promptPID",
        "frontmostAttribute(for: promptPID) == false",
        "\"appSharePromptBundleMatched\": appSharePrompt.bundleMatched",
        "\"appSharePromptButtonEnabledMatched\": appSharePrompt.buttonEnabledMatched",
    ] {
        assert!(
            probe.contains(required),
            "SystemProbe omits exact app/window/button proof: {required}"
        );
    }
    for raw_identity in [
        "\"appSharePromptPID\":",
        "\"appSharePromptTitle\":",
        "\"appSharePromptBundleIdentifier\":",
    ] {
        assert!(
            !probe.contains(raw_identity),
            "SystemProbe must emit bounded app-share booleans only: {raw_identity}"
        );
    }

    for required in [
        "const APP_SHARE_HANDOFF_MARKER_SCHEMA = 2;",
        "const APP_SHARE_HANDOFF_REQUEST_FILE = \"macos-app-share-concurrency-handoff-request.json\";",
        "const APP_SHARE_HANDOFF_START_FILE = \"macos-app-share-concurrency-handoff-start.json\";",
        "const APP_SHARE_HANDOFF_COMPLETE_FILE = \"macos-app-share-concurrency-handoff-complete.json\";",
        "const APP_SHARE_BUNDLE_IDENTIFIER = \"dev.flrngel.local-browser-bridge.acceptance.app-share\";",
        "const APP_SHARE_WINDOW_TITLE = \"LBB macOS Acceptance App Share\";",
        "const APP_SHARE_READY_BUTTON_TEXT = \"START APP-SHARE CHECK\";",
        "expectedButtonAccessibilityIdentifier: \"lbb-app-share-start\"",
        "exactAppShareRequired: true",
        "physicalHumanProvenanceRequired: false",
        "notificationOnly: false",
        "acceptedAsProductAuthority: false",
        "async function readBoundAppShareReceipt(path, expectedKeys, validate, description)",
        "fsConstants.O_NONBLOCK",
        "sameOrdinaryFileIdentity(descriptorBefore, descriptorAfter)",
        "runAppShareFilesystemSelfTests",
        "record.kind === \"macos-app-share-concurrency-handoff-start\"",
        "record.buttonActionObserved === true",
        "record.requestSha256 === pointerHandoffRequestSha256",
        "record.kind === \"macos-app-share-concurrency-handoff-complete\"",
        "record.startReceiptSha256 === pointerHandoffStartReceiptSha256",
        "record.productActionStartedAt === pointerHandoffProductActionStartedAt",
        "record.productActionCompletedAt === pointerHandoffProductActionCompletedAt",
        "function runExactLine(commandName, args, expectedLine",
        "result.stderr === \"\" && result.stdout === `${expectedLine}\\n`",
        "extra subprocess stdout was accepted",
        "subprocess stdout without its exact trailing LF was accepted",
        "subprocess stderr was accepted",
        "nonzero subprocess exit was accepted",
        "independentPointerLaneState(appShareActionInvariants) === \"quiet\"",
        "independentPointerLaneState(actionTransitionInvariants) === \"quiet\"",
        "pointerHandoffSharedHidInputObserved = false",
        "pointerHandoffSurfaceObservedAtProductBoundaries = true",
        "appShareSurfaceObservedAtProductBoundaries: pointerHandoffSurfaceObservedAtProductBoundaries",
        "physicalHumanProvenanceClaimed: false",
        "cryptographicToolIdentityClaimed: false",
        "orchestrationNotProductControl: true",
        "markerAcceptedAsProductAuthority: false",
        "macOS packaged-evidence rig self-test passed.",
    ] {
        assert!(
            rig.contains(required),
            "exact app-share release contract is missing: {required}"
        );
    }
    assert_eq!(
        app.matches("print(").count(),
        1,
        "app-share self-test must expose only its canonical success line"
    );
    for forbidden in [
        "run(pointerHandoffBinary, [\"--self-test\"])",
        "externalAcknowledgementConsumed: true",
        "markerAcceptedAsProductAuthority: true",
        "physicalHumanProvenanceClaimed: true",
        "cryptographicToolIdentityClaimed: true",
    ] {
        assert!(
            !rig.contains(forbidden),
            "app-share marker must not become product authority: {forbidden}"
        );
    }

    let request = rig
        .find("const requestMarker = pointerHandoffRequestMarker")
        .unwrap();
    let request_publish = rig
        .find("await publishAtomicMarkerOnce(\n    pointerHandoffRequestPath")
        .unwrap();
    let start_receipt = rig
        .find("const startReceipt = await waitFor(\n    \"fresh exact-app-share start receipt\"")
        .unwrap();
    let product_started = rig
        .find("pointerHandoffProductActionStartedAt = new Date().toISOString();")
        .unwrap();
    let product_action = rig
        .find("const postResizeClickResponse = await commandResponse(\n    \"computer.click\"")
        .unwrap();
    let effect_proof = rig
        .find("pointerHandoffTargetPostconditionObserved = true;")
        .unwrap();
    let completion = rig
        .find("await completePointerHandoff(postResizeSystemAfter)")
        .unwrap();
    assert!(
        request < request_publish
            && request_publish < start_receipt
            && start_receipt < product_started
            && product_started < product_action
            && product_action < effect_proof
            && effect_proof < completion,
        "request/start/product/postcondition/completion chain is out of order"
    );
    let complete_function_start = rig.find("async function completePointerHandoff(").unwrap();
    let complete_function_end = rig[complete_function_start..]
        .find("\nconst FIXTURE_ACTIONS")
        .map(|offset| complete_function_start + offset)
        .unwrap();
    let complete_function = &rig[complete_function_start..complete_function_end];
    let complete_control = complete_function
        .find("await writePointerHandoffState(POINTER_HANDOFF_COMPLETE_STATE")
        .unwrap();
    let complete_receipt = complete_function
        .find("\"bound app-share completion receipt\"")
        .unwrap();
    assert!(
        complete_control < complete_receipt,
        "completion receipt wait must follow the bound COMPLETE control publication"
    );

    for required in [
        "exact bundle",
        "one stable nonactivating window",
        "START APP-SHARE CHECK",
        "request",
        "start",
        "complete",
        "notification-only",
        "not product authority",
        "does not prove",
        "independent input seat",
    ] {
        assert!(
            normalized_readme.contains(required),
            "exact app-share boundary is undocumented: {required}"
        );
    }
}

#[test]
fn macos_v0_12_27_refreshes_post_handoff_share_action_authority_before_click() {
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let computer = fs::read_to_string("src/computer.rs")
        .unwrap()
        .replace("\r\n", "\n");

    assert!(computer.contains("capture_publication_metadata(captured_at, now_iso)"));
    assert!(
        computer.contains("capture_publication_metadata_samples_wall_time_before_monotonic_age")
    );

    let screenshot_mapper_start = rig
        .find("async function saveCurrentScreenshot(publicState, filename, expectedWindowId)")
        .unwrap();
    let screenshot_mapper_end = rig[screenshot_mapper_start..]
        .find("\nfunction actionSummary(body)")
        .map(|offset| screenshot_mapper_start + offset)
        .unwrap();
    let screenshot_mapper = &rig[screenshot_mapper_start..screenshot_mapper_end];
    assert!(
        !screenshot_mapper.contains("capturedAt: observation.capturedAt")
            && !screenshot_mapper.contains("captureAgeMs: observation.captureAgeMs"),
        "retained screenshot records must stay within the finalizer's closed schema"
    );

    let mapper_start = rig
        .find("function shareActionAuthority(observation, sample)")
        .unwrap();
    let mapper_end = rig[mapper_start..]
        .find("\n}\n\nfunction exactShareActionAuthorityAdvanced")
        .map(|offset| mapper_start + offset)
        .unwrap();
    let mapper = &rig[mapper_start..mapper_end];
    for required in [
        "capturedAt: observation.capturedAt",
        "captureAgeMs: observation.captureAgeMs",
    ] {
        assert!(
            mapper.contains(required),
            "share-action authority mapper omits {required}"
        );
    }

    for required in [
        "function shareActionAuthority(observation, sample)",
        "shareAuthorityBaseline = shareActionAuthority({",
        "share action authority mapper self-test failed",
        "function exactShareActionAuthorityAdvanced(reference, candidate)",
        "candidate.frameId !== reference.frameId",
        "reference.topLevelShareId === reference.shareId",
        "candidate.topLevelShareId === candidate.shareId",
        "candidate.sourceSequence > reference.sourceSequence",
        "candidate.topLevelSourceSequence === candidate.sourceSequence",
        "candidate.sequence > reference.sequence",
        "candidate.windowId === reference.windowId",
        "candidate.pid === reference.pid",
        "candidate.windowTitle === reference.windowTitle",
        "candidate.windowWidth === reference.windowWidth",
        "candidate.windowHeight === reference.windowHeight",
        "candidate.imageWidth === reference.imageWidth",
        "candidate.imageHeight === reference.imageHeight",
        "function estimatedShareActionAuthorityAgeMs(authority, nowMilliseconds = Date.now())",
        "function decideShareActionAuthorityRefresh(reference, candidate, nowMilliseconds)",
        "function decideShareActionAuthorityDispatch(",
        "exactShareActionAuthorityAdvanced(reference, authority)",
        "const APP_SHARE_HANDOFF_COMPLETION_GRACE_MS = 18_000;",
        "const APP_SHARE_HANDOFF_MINIMUM_ACTION_BUDGET_MS = 8_000;",
        "const APP_SHARE_HANDOFF_COMPLETION_RESERVE_MS = 3_000;",
        "const APP_SHARE_ACTION_AUTHORITY_REFRESH_MAXIMUM_WAIT_MS = 5_000;",
        "const APP_SHARE_ACTION_FRAME_MAX_ESTIMATED_AGE_MS = 2_500;",
        "estimatedAgeMs <= APP_SHARE_ACTION_FRAME_MAX_ESTIMATED_AGE_MS",
        "async function waitForFreshShareActionAuthority({",
        "monotonicMilliseconds = () => performance.now()",
        "wallClockMilliseconds = () => Date.now()",
        "signal: readAbortController.signal",
        "monotonicMilliseconds() >= deadlineMilliseconds",
        "async function apiState(signal = null)",
        "loadCandidate: async ({ signal })",
        "const snapshot = await apiState(signal);",
        "fresh share action authority timed out:",
        "minimumEstimatedAgeMs=",
        "rejectionCounts=",
        "function freshShareActionAuthorityTimeout(remainingMilliseconds)",
        "APP_SHARE_HANDOFF_COMPLETION_RESERVE_MS",
        "APP_SHARE_HANDOFF_MINIMUM_ACTION_BUDGET_MS",
        "APP_SHARE_ACTION_AUTHORITY_REFRESH_MAXIMUM_WAIT_MS",
        "post-handoff share action authority refresh self-test failed",
        "valid exact 2.5s successor and 1250ms-age authority accepted",
        "2501ms-age",
        "stalled read abort",
        "late response refusal",
        "monotonic deadline aborts stalled reads and refuses late fresh responses.",
        "app-share receipt retained the exact persistent share",
        "share-after-app-share-action-authority",
        "post-handoff share action authority is fresh and exact",
        "post-handoff share action authority remained fresh at dispatch",
        "app-share handoff and frame refresh caused no target mutation",
        "pointerHandoffAuthorityRefreshedAfterReceipt = true",
        "pointerHandoffAuthorityFreshAtDispatch = true",
        "authorityRefreshedAfterReceipt: pointerHandoffAuthorityRefreshedAfterReceipt",
        "authorityFreshAtDispatch: pointerHandoffAuthorityFreshAtDispatch",
        "cross-share dispatch authority",
        "cross-target dispatch authority",
        "dispatch geometry race",
        "insufficient bounded time remained to refresh share action authority",
    ] {
        assert!(
            rig.contains(required),
            "post-handoff share-authority contract omits {required}"
        );
    }

    let action_receipt = rig
        .find("actionPromptBaseline = await waitForDeliberatePointerActivity(armedBaseline);")
        .unwrap();
    let read_only_barrier = rig[action_receipt..]
        .find("const armedShareSnapshot = await apiState();")
        .map(|offset| action_receipt + offset)
        .unwrap();
    let strict_successor = rig[read_only_barrier..]
        .find("current = await waitForFreshShareActionAuthority({\n      reference: armedShareAuthority,")
        .map(|offset| read_only_barrier + offset)
        .unwrap();
    let authority_assignment = rig[strict_successor..]
        .find("const beforePostResizeAction = current.sample;")
        .map(|offset| strict_successor + offset)
        .unwrap();
    let observation_assignment = rig[authority_assignment..]
        .find("observation = current.snapshot.computerObservation;")
        .map(|offset| authority_assignment + offset)
        .unwrap();
    let action_baseline = rig[observation_assignment..]
        .find("stage: \"postResizePixelAction\"")
        .map(|offset| observation_assignment + offset)
        .unwrap();
    let fixture_recheck = rig[strict_successor..]
        .find("postResizeFixtureBefore = await fixtureState(fixtureStatePath);")
        .map(|offset| strict_successor + offset)
        .unwrap();
    let dispatch_decision = rig[action_baseline..]
        .find("const dispatchAuthorityDecision = decideShareActionAuthorityDispatch(")
        .map(|offset| action_baseline + offset)
        .unwrap();
    let dispatch_age_guard = rig[action_baseline..]
        .find("post-handoff share action authority remained fresh at dispatch")
        .map(|offset| action_baseline + offset)
        .unwrap();
    let product_action = rig[action_baseline..]
        .find("const postResizeClickResponse = await commandResponse(\n    \"computer.click\"")
        .map(|offset| action_baseline + offset)
        .unwrap();
    assert!(
        action_receipt < read_only_barrier
            && read_only_barrier < strict_successor
            && strict_successor < authority_assignment
            && authority_assignment < observation_assignment
            && observation_assignment < action_baseline
            && strict_successor < fixture_recheck
            && fixture_recheck < dispatch_age_guard
            && action_baseline < dispatch_decision
            && dispatch_decision < dispatch_age_guard
            && dispatch_age_guard < product_action,
        "ACTION receipt, read-only barrier, strict successor, fresh authority, and click are out of order"
    );

    let rebase_segment = &rig[read_only_barrier..product_action];
    assert!(
        !rebase_segment.contains("computer.observe"),
        "live-share action rebasing must not race the stream with a one-shot observe"
    );
    assert!(rebase_segment.contains(
        "const postResizeSystemBefore = pointerEvidenceLane === \"deliberate-concurrency\"\n    ? actionPromptBaseline"
    ));
    let dispatch_guard_segment = &rig[dispatch_decision..product_action];
    for required in [
        "const dispatchAuthorityDecision = decideShareActionAuthorityDispatch(",
        "receiptBoundShareActionAuthority,",
        "postHandoffShareActionAuthority,",
        "observation.frameId,",
        "dispatchAuthorityDecision.accepted",
        "pointerHandoffAuthorityFreshAtDispatch = true;",
    ] {
        assert!(
            dispatch_guard_segment.contains(required),
            "live dispatch authority guard omits {required}"
        );
    }
}

#[test]
fn macos_pointer_arm_state_machine_execution_regressions_pass() {
    let output = match Command::new("node")
        .args([
            "evidence/v0.12.59/computer/helper-evidence-rig.mjs",
            "--self-test",
        ])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to execute macOS pointer-arm self-test: {error}"),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "macOS pointer-arm execution self-test failed:\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stdout.contains(
        "macOS pointer-arm execution regressions passed: deadline expiry, MOVE re-arm, final ACTION re-arm."
    ));
    assert!(stdout.contains(
        "Post-handoff share action authority regressions passed: valid exact 2.5s successor and 1250ms-age authority accepted; 2501ms-age, wrong-share, wrong-target, changed-geometry, replay, and no-successor dispatch refused with reserved budgets; monotonic deadline aborts stalled reads and refuses late fresh responses."
    ));
    assert!(stdout.contains(
        "Cross-platform marker identity regressions passed: exact BigInt binding, Windows metadata projection, POSIX private mode."
    ));
    assert!(stdout.contains("macOS packaged-evidence rig self-test passed."));
}

#[test]
fn v0_12_27_marker_identity_is_exact_on_windows_and_posix() {
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "function hasExactBigIntFileIdentity(state)",
        "typeof state?.dev === \"bigint\" && state.dev > 0n",
        "typeof state.ino === \"bigint\" && state.ino > 0n",
        "typeof state.mtimeNs === \"bigint\" && typeof state.ctimeNs === \"bigint\"",
        "function samePersistentFileObjectIdentity(left, right)",
        "function sameOrdinaryFileIdentity(left, right)",
        "function samePublishedFileAcrossWriterClose(left, right, platform = process.platform)",
        "left.dev === right.dev && left.ino === right.ino && left.size === right.size",
        "left.mode === right.mode && left.uid === right.uid && left.gid === right.gid",
        "left.birthtimeNs === right.birthtimeNs",
        "left.mtimeNs === right.mtimeNs",
        "left.ctimeNs === right.ctimeNs",
        "left.nlink === right.nlink",
        "!samePersistentFileObjectIdentity(left, right) || left.nlink !== right.nlink",
        "handle.stat({ bigint: true })",
        "lstat(temporaryPath, { bigint: true })",
        "lstat(path, { bigint: true })",
        "function hasPlatformPrivateMarkerMetadata(",
        "if (platform === \"win32\")",
        "projectedPermissions === 0o444n || projectedPermissions === 0o666n",
        "state.uid === 0n && state.gid === 0n",
        "currentUid !== null && state.uid === currentUid",
        "(state.mode & 0o077n) === 0n",
        "async function syncMarkerDirectory(path, deadlineMilliseconds, platform = process.platform)",
        "operator marker linked descriptor inspection",
        "sameCoreFileIdentity(descriptorState, linkedDescriptorState)",
        "operator marker post-close identity inspection",
        "settledPublishedState.nlink !== 1n",
        "samePublishedFileAcrossWriterClose(publishedState, settledPublishedState)",
        "operator marker published-file descriptor inspection",
        "publishedBytes.equals(Buffer.from(serialized, \"utf8\"))",
        "sameOrdinaryFileIdentity(settledPublishedState, reboundDescriptorState)",
        "operator marker published path lost its stable ordinary-file binding",
    ] {
        assert!(
            rig.contains(required),
            "cross-platform marker identity contract omits {required}"
        );
    }
    for forbidden in [
        "process.platform === \"win32\" || (state.mode & 0o077n) === 0n",
        "if (platform === \"win32\") return true",
        "descriptorState.mode & 0o077) !== 0",
        "handle.stat(),",
    ] {
        assert!(
            !rig.contains(forbidden),
            "marker identity contract contains a lossy or global Windows bypass: {forbidden}"
        );
    }

    let publisher = rig
        .split_once("async function publishAtomicMarkerOnce")
        .unwrap()
        .1
        .split_once("async function writePointerHandoffState")
        .unwrap()
        .0;
    assert_eq!(
        publisher
            .matches("samePublishedFileAcrossWriterClose(")
            .count(),
        1,
        "the Windows close-time exception must be confined to one publication boundary"
    );
    let receipt_reader = rig
        .split_once("async function readBoundAppShareReceipt")
        .unwrap()
        .1
        .split_once("async function readAppShareStartReceipt")
        .unwrap()
        .0;
    assert!(!receipt_reader.contains("samePublishedFileAcrossWriterClose("));
    assert!(
        receipt_reader.matches("sameOrdinaryFileIdentity(").count() >= 3,
        "receipt reads must keep strict descriptor/path/timestamp equality"
    );
}

#[test]
fn withdrawn_macos_v0_12_10_evidence_remains_byte_exact() {
    let root =
        "evidence/v0.12.10/computer/attempts/withdrawn-de59840-macos-deliberate-pointer-timeout";
    for (name, expected) in [
        (
            "README.md",
            "9cb8a4b18bf4b98931b123597db5d60519708900fffcbc25b573ee57d0fc5fce",
        ),
        (
            "computer-01-exact-window-observe.png",
            "ec4e34ef15cc8e2007523949dafaef1cf6ebe30bd5e309614a43b202eb5d436b",
        ),
        (
            "computer-02-semantic-set-value.png",
            "7d1111f4c65763be9769b8e5ff3e681de1ab44737251e8d1f938086f80e75d97",
        ),
        (
            "computer-03-semantic-invoke.png",
            "f3efb1d688aeb472efe9cdd2112030083eaf34dccca6bc5b11b22ee0235bc002",
        ),
        (
            "computer-04-persistent-scstream-start.png",
            "0f2fe8b49c242262cf0d1e2088924b712874fd3f8c9ca151c7f38ec2e5900d7b",
        ),
        (
            "computer-05-live-share-pixel-action.png",
            "5e05b1165715b234bd527c8039ed11f804ce9c7852e706fd30fa3216a2454e6a",
        ),
        (
            "computer-06-persistent-share-resize.png",
            "474c6f215288717e053d00a0cbfc60227733e1802d8b9b51467a00b214cf01ff",
        ),
        (
            "helper-results.json",
            "70794822e86c9749bac064c011377fc6c87ef387834c91250a92ec6c9f9e2fc2",
        ),
        (
            "helper-rig.log",
            "12db1c5278c7b6232c24aaf3a7d69b89cad5882d72d20468ef2373eb268dd50c",
        ),
    ] {
        let path = format!("{root}/{name}");
        assert_eq!(
            file_sha256(&path),
            expected,
            "withdrawn v0.12.10 evidence changed: {path}"
        );
    }
}

#[test]
fn historical_macos_v0_12_1_and_v0_12_6_evidence_sources_remain_byte_exact() {
    for (path, expected) in [
        (
            "evidence/v0.12.6/computer/README.md",
            "a70041c5328bf49a67999eeb6d5c4dad93876256f1959bcd4474440b1bd28cb9",
        ),
        (
            "evidence/v0.12.6/computer/HelperEvidenceFixture.swift",
            "b13e20bb6a5bc7bb0b5c74a57dc13d8bd24e60e99cad11c66dd2a23596261206",
        ),
        (
            "evidence/v0.12.6/computer/SystemProbe.swift",
            "c9a46e556b87d21bb7db414ba6fa00457928c8ab661be6d596a4fe650ddf6dde",
        ),
        (
            "evidence/v0.12.6/computer/helper-evidence-rig.mjs",
            "a930d0d3beab1448850ec996114483461d38b2aaac120e57eb2d266d3884521a",
        ),
        (
            "evidence/v0.12.1/computer/HelperEvidenceFixture.swift",
            "5a99ad27d5bc80388a8697c0aac9b3c3b4af25e5840aee9e32a3cb9a8c7142ff",
        ),
        (
            "evidence/v0.12.1/computer/SystemProbe.swift",
            "c9a46e556b87d21bb7db414ba6fa00457928c8ab661be6d596a4fe650ddf6dde",
        ),
        (
            "evidence/v0.12.1/computer/helper-evidence-rig.mjs",
            "cac8ae8daeedce4f8d6a8cf440ccc5161637a2486f4e8cccca62c29a393883cc",
        ),
        (
            "evidence/v0.12.1/computer/README.md",
            "b9e91e9f3f9940a0c55b7d86c02cc97d33b0b50a6eb5bae716d2fa08c55d72a1",
        ),
    ] {
        assert_eq!(
            file_sha256(path),
            expected,
            "historical evidence changed: {path}"
        );
    }
}

#[test]
fn historical_macos_v0_12_2_evidence_sources_remain_byte_exact() {
    for (path, expected) in [
        (
            "evidence/v0.12.2/computer/HelperEvidenceFixture.swift",
            "dbb0f52af64b973838160870cb4d683d06ffb1001ae432c93bc2993cafa11684",
        ),
        (
            "evidence/v0.12.2/computer/SystemProbe.swift",
            "c9a46e556b87d21bb7db414ba6fa00457928c8ab661be6d596a4fe650ddf6dde",
        ),
        (
            "evidence/v0.12.2/computer/helper-evidence-rig.mjs",
            "7aff7c985d2e1c6fecec4f83b1c06dc28f1cc85b46187ae9588f69af770fbc21",
        ),
        (
            "evidence/v0.12.2/computer/README.md",
            "e3308ea36936d16fe42bfa4dde345786ca99324162c8d710a069c5774ea49db5",
        ),
    ] {
        assert_eq!(
            file_sha256(path),
            expected,
            "historical evidence changed: {path}"
        );
    }
}

#[test]
fn withdrawn_v0_12_6_exact_candidate_outputs_remain_byte_exact() {
    for (path, expected) in [
        (
            "evidence/v0.12.6/computer/computer-01-exact-window-observe.png",
            "70312651ac419f99bfa4c9ff98ca79ed5ea0ff7d16bc0441171b75d9f5f34ea1",
        ),
        (
            "evidence/v0.12.6/computer/computer-02-semantic-set-value.png",
            "cf5eef8e072cb0e08e1012c5e88e7d167a9da06cc1b036aa82ecc7ac972ec017",
        ),
        (
            "evidence/v0.12.6/computer/computer-03-semantic-invoke.png",
            "5625ce884fffffe5550becd637ae3434728defc60ef5025e9bcbff812d0038ef",
        ),
        (
            "evidence/v0.12.6/computer/computer-04-persistent-scstream-start.png",
            "fa9a3c4130b289ef4b997dca3a4256b26e45fd734b16fc78d375750ec69d8b65",
        ),
        (
            "evidence/v0.12.6/computer/computer-05-live-share-pixel-action.png",
            "222d3380b48d7d5723e4f7f30e69d21219cdda94c368af83ee02018f337d62a2",
        ),
        (
            "evidence/v0.12.6/computer/computer-06-persistent-share-resize.png",
            "34926d4f91567385b5922d89db5249a1f83a0f45f66a0367d8b9ffa21156a629",
        ),
        (
            "evidence/v0.12.6/computer/helper-results.json",
            "8347a8161114b54a596ee76fd0372388e3ef920212188814dc40d9402359b646",
        ),
        (
            "evidence/v0.12.6/computer/helper-rig.log",
            "a8a52ee3af1aff13c21c8e65dc4308b9354b0aec6a9b4b14804ecd6cc78b4757",
        ),
        (
            "evidence/v0.12.6/computer/attempts/withdrawn-397e4b6-windows-foreground-sentinel-timeout/fixture/fixture-events.ndjson",
            "7a9d20f90c1ae7ec0badc7c28e40a0946b21622f79b92be05f6f99d67ee60a70",
        ),
        (
            "evidence/v0.12.6/computer/attempts/withdrawn-397e4b6-windows-foreground-sentinel-timeout/fixture/fixture-ready.json",
            "800549e7281115d79cc933c98b95d61360e0e6dc6582fee93b22e3e737b389c1",
        ),
        (
            "evidence/v0.12.6/computer/attempts/withdrawn-397e4b6-windows-foreground-sentinel-timeout/fixture/fixture-state.json",
            "f7f71e78deaf8854070d94d0afc8b1d1b9e61fe641646bdd5b9442094e8bee7c",
        ),
        (
            "evidence/v0.12.6/computer/attempts/withdrawn-397e4b6-windows-foreground-sentinel-timeout/steps/01-protocol-bound-helper-readiness.json",
            "2719e172911aa00201b5b257e7c51a52279ab76b24e8aea9b2e3bc223d1ae84f",
        ),
        (
            "evidence/v0.12.6/computer/attempts/withdrawn-397e4b6-windows-foreground-sentinel-timeout/summary.json",
            "c44743bdb1b805aacedf1faa555729bd0f8c36c4fdc96e46e4d0fd4cc88ab4ff",
        ),
        (
            "evidence/v0.12.6/computer/attempts/withdrawn-397e4b6-windows-foreground-sentinel-timeout/README.md",
            "7ba077b172949dd4be1fc23090592b300d143606aa163f04dbb76a93c6c981a6",
        ),
    ] {
        assert_eq!(
            file_sha256(path),
            expected,
            "withdrawn exact-candidate evidence changed: {path}"
        );
    }

    let macos_root_entries = fs::read_dir("evidence/v0.12.6/computer")
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| {
            let mut name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().unwrap().is_dir() {
                name.push('/');
            }
            name
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        macos_root_entries,
        BTreeSet::from([
            "HelperEvidenceFixture.swift".to_owned(),
            "README.md".to_owned(),
            "SystemProbe.swift".to_owned(),
            "attempts/".to_owned(),
            "computer-01-exact-window-observe.png".to_owned(),
            "computer-02-semantic-set-value.png".to_owned(),
            "computer-03-semantic-invoke.png".to_owned(),
            "computer-04-persistent-scstream-start.png".to_owned(),
            "computer-05-live-share-pixel-action.png".to_owned(),
            "computer-06-persistent-share-resize.png".to_owned(),
            "helper-results.json".to_owned(),
            "helper-rig.log".to_owned(),
            "helper-evidence-rig.mjs".to_owned(),
        ])
    );

    let results: serde_json::Value = serde_json::from_str(
        &fs::read_to_string("evidence/v0.12.6/computer/helper-results.json").unwrap(),
    )
    .unwrap();
    for (field, path) in [
        (
            "runnerSha256",
            "evidence/v0.12.6/computer/helper-evidence-rig.mjs",
        ),
        (
            "fixtureSha256",
            "evidence/v0.12.6/computer/HelperEvidenceFixture.swift",
        ),
        (
            "systemProbeSha256",
            "evidence/v0.12.6/computer/SystemProbe.swift",
        ),
    ] {
        assert_eq!(
            results["harness"][field].as_str().unwrap(),
            file_sha256(path),
            "retained harness binding drifted for {path}"
        );
    }

    fn collect_relative_files(
        root: &std::path::Path,
        current: &std::path::Path,
        out: &mut BTreeSet<String>,
    ) {
        for entry in fs::read_dir(current).unwrap().map(Result::unwrap) {
            if entry.file_type().unwrap().is_dir() {
                collect_relative_files(root, &entry.path(), out);
            } else {
                out.insert(
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let windows_root = std::path::Path::new(
        "evidence/v0.12.6/computer/attempts/withdrawn-397e4b6-windows-foreground-sentinel-timeout",
    );
    let mut windows_files = BTreeSet::new();
    collect_relative_files(windows_root, windows_root, &mut windows_files);
    assert_eq!(
        windows_files,
        BTreeSet::from([
            "README.md".to_owned(),
            "fixture/fixture-events.ndjson".to_owned(),
            "fixture/fixture-ready.json".to_owned(),
            "fixture/fixture-state.json".to_owned(),
            "steps/01-protocol-bound-helper-readiness.json".to_owned(),
            "summary.json".to_owned(),
        ])
    );
}

#[test]
fn withdrawn_v0_12_7_exact_candidate_outputs_remain_byte_exact() {
    for (path, expected) in [
        (
            "evidence/v0.12.7/computer/README.md",
            "62abf279ddd03191f6736a7852866c293de756a410da452d5d683ced019b432e",
        ),
        (
            "evidence/v0.12.7/computer/HelperEvidenceFixture.swift",
            "ce7b88b1eafba7c284b348116937e103991af1b5460a653ff1e3580c925777db",
        ),
        (
            "evidence/v0.12.7/computer/SystemProbe.swift",
            "c9a46e556b87d21bb7db414ba6fa00457928c8ab661be6d596a4fe650ddf6dde",
        ),
        (
            "evidence/v0.12.7/computer/helper-evidence-rig.mjs",
            "638d77ab756d11ac1a46fe7b66aa0477a62f70647610889d76d6bd1f11bb562d",
        ),
        (
            "evidence/v0.12.7/computer/helper-results.json",
            "adacc4548b792df5dc278c7e5509b2f35814940a67ca0f377ff6e04cdac1572b",
        ),
        (
            "evidence/v0.12.7/computer/helper-rig.log",
            "cbd81926825a1403a25000abfac5b2a476a1412f06174724534cf412ec425a53",
        ),
        (
            "evidence/v0.12.7/computer/computer-01-exact-window-observe.png",
            "54124e0ba163dccdf45bf3988b72d852fc176476431f6415493c865ba15736d0",
        ),
        (
            "evidence/v0.12.7/computer/computer-02-semantic-set-value.png",
            "8c6b062d0525fce8d89d77d1f0c9b12a27e0aec677a3692599c866924e232aad",
        ),
        (
            "evidence/v0.12.7/computer/computer-03-semantic-invoke.png",
            "6526ae14e01fdb25b0830f0092525d2c12c41d0ae8e0229bbbc640e60820fd41",
        ),
        (
            "evidence/v0.12.7/computer/computer-04-persistent-scstream-start.png",
            "db040b9454c4016cb827649f535f2da6e0d658ea8c2ffc97b9410f6c86fdd5b5",
        ),
        (
            "evidence/v0.12.7/computer/computer-05-live-share-pixel-action.png",
            "74268d4fe60e568ff58c2a3f42366c0a7efecda66a20c9928fe791c23c0933ee",
        ),
        (
            "evidence/v0.12.7/computer/computer-06-persistent-share-resize.png",
            "46daeff4a950a76f56a6ae7baf381393d1c11af3b295c16b049b32f8742903ab",
        ),
        (
            "evidence/v0.12.7/computer/attempts/withdrawn-0749953-windows-foreground-arm-timeout/README.md",
            "6d10aa26dc7f6a7e4e86313b2eee805a3e2a68fc4facc866bb21b0e06320511f",
        ),
        (
            "evidence/v0.12.7/computer/attempts/withdrawn-0749953-windows-foreground-arm-timeout/fixture/fixture-events.ndjson",
            "d8bb290dcddeacd7dffb7c9f985096791ee048bcf6014038755e1a12df8476a0",
        ),
        (
            "evidence/v0.12.7/computer/attempts/withdrawn-0749953-windows-foreground-arm-timeout/fixture/fixture-ready.json",
            "1903aa7c34f3ad9c4088b4285e7cd5d66a1bf8a447f9c2ff9b76d553559e2d5f",
        ),
        (
            "evidence/v0.12.7/computer/attempts/withdrawn-0749953-windows-foreground-arm-timeout/fixture/fixture-state.json",
            "f9880cb98a1a7a9a686ec0f6c011f0664a1d0529408893d38ca3f170ce350cf8",
        ),
        (
            "evidence/v0.12.7/computer/attempts/withdrawn-0749953-windows-foreground-arm-timeout/steps/01-protocol-bound-helper-readiness.json",
            "9ee1dd964734eda9d216acbc8b955b172d12fe0707ae38a3f9de31b854b39865",
        ),
        (
            "evidence/v0.12.7/computer/attempts/withdrawn-0749953-windows-foreground-arm-timeout/steps/02-foreground-arm-request-delivery.json",
            "391e7573abd178bd1bafccedbf235fd4c1d2d72d9f09402ed9d5cc5564f20900",
        ),
        (
            "evidence/v0.12.7/computer/attempts/withdrawn-0749953-windows-foreground-arm-timeout/summary.json",
            "639179342894f075f672e18bbbd7c62865bf842cc0751a10bd97a4ddc4ce1f95",
        ),
    ] {
        assert_eq!(
            file_sha256(path),
            expected,
            "withdrawn v0.12.7 exact-candidate evidence changed: {path}"
        );
    }

    let macos_root_entries = fs::read_dir("evidence/v0.12.7/computer")
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| {
            let mut name = entry.file_name().to_string_lossy().into_owned();
            if entry.file_type().unwrap().is_dir() {
                name.push('/');
            }
            name
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        macos_root_entries,
        BTreeSet::from([
            "HelperEvidenceFixture.swift".to_owned(),
            "README.md".to_owned(),
            "SystemProbe.swift".to_owned(),
            "attempts/".to_owned(),
            "computer-01-exact-window-observe.png".to_owned(),
            "computer-02-semantic-set-value.png".to_owned(),
            "computer-03-semantic-invoke.png".to_owned(),
            "computer-04-persistent-scstream-start.png".to_owned(),
            "computer-05-live-share-pixel-action.png".to_owned(),
            "computer-06-persistent-share-resize.png".to_owned(),
            "helper-evidence-rig.mjs".to_owned(),
            "helper-results.json".to_owned(),
            "helper-rig.log".to_owned(),
        ])
    );

    let results: serde_json::Value = serde_json::from_str(
        &fs::read_to_string("evidence/v0.12.7/computer/helper-results.json").unwrap(),
    )
    .unwrap();
    assert_eq!(results["status"], "passed-release-candidate");
    assert_eq!(results["assertions"]["passed"], 187);
    assert_eq!(results["assertions"]["failed"], 0);
    assert_eq!(results["screenshots"].as_array().unwrap().len(), 6);
    for (field, path) in [
        (
            "runnerSha256",
            "evidence/v0.12.7/computer/helper-evidence-rig.mjs",
        ),
        (
            "fixtureSha256",
            "evidence/v0.12.7/computer/HelperEvidenceFixture.swift",
        ),
        (
            "systemProbeSha256",
            "evidence/v0.12.7/computer/SystemProbe.swift",
        ),
    ] {
        assert_eq!(
            results["harness"][field].as_str().unwrap(),
            file_sha256(path),
            "retained v0.12.7 harness binding drifted for {path}"
        );
    }

    fn collect_relative_files(
        root: &std::path::Path,
        current: &std::path::Path,
        out: &mut BTreeSet<String>,
    ) {
        for entry in fs::read_dir(current).unwrap().map(Result::unwrap) {
            if entry.file_type().unwrap().is_dir() {
                collect_relative_files(root, &entry.path(), out);
            } else {
                out.insert(
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let windows_root = std::path::Path::new(
        "evidence/v0.12.7/computer/attempts/withdrawn-0749953-windows-foreground-arm-timeout",
    );
    let mut windows_files = BTreeSet::new();
    collect_relative_files(windows_root, windows_root, &mut windows_files);
    assert_eq!(
        windows_files,
        BTreeSet::from([
            "README.md".to_owned(),
            "fixture/fixture-events.ndjson".to_owned(),
            "fixture/fixture-ready.json".to_owned(),
            "fixture/fixture-state.json".to_owned(),
            "steps/01-protocol-bound-helper-readiness.json".to_owned(),
            "steps/02-foreground-arm-request-delivery.json".to_owned(),
            "summary.json".to_owned(),
        ])
    );

    let summary: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(windows_root.join("summary.json")).unwrap())
            .unwrap();
    assert_eq!(summary["passed"], false);
    assert_eq!(summary["failureDetails"]["stage"], "wait-foreground-arm");
    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(windows_root.join("fixture/fixture-state.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(state["foregroundArmRequestCount"], 1);
    assert_eq!(state["foregroundArmAcknowledgementCount"], 0);
    assert_eq!(state["foregroundArmLeftMouseDownCount"], 0);
    assert_eq!(state["foregroundArmLeftMouseUpCount"], 0);
    assert_eq!(state["foregroundArmButtonEnabled"], true);
}

#[test]
fn withdrawn_v0_12_8_exact_candidate_results_remain_byte_exact_and_fail_closed() {
    for (path, expected) in [
        (
            "evidence/v0.12.8/computer/HelperEvidenceFixture.swift",
            "8f7692964bad4834edb61a3d13ecc3889f6c6050aa2abfe48806cbfa149a6c9f",
        ),
        (
            "evidence/v0.12.8/computer/SystemProbe.swift",
            "c9a46e556b87d21bb7db414ba6fa00457928c8ab661be6d596a4fe650ddf6dde",
        ),
        (
            "evidence/v0.12.8/computer/helper-evidence-rig.mjs",
            "cedd30105136527aa238557c8216103480d3c39ffab599a8b3c8cd38e563ba92",
        ),
        (
            "evidence/v0.12.8/computer/helper-results.json",
            "571ab9447a322818d8fbf0a1c97607514fca0361488abe1ee924f3d0af621b94",
        ),
        (
            "evidence/v0.12.8/computer/helper-rig.log",
            "0599c1267e1067a9755dc237a47c83b7f65bea1ef20611f164064ed638b1f78e",
        ),
        (
            "evidence/v0.12.8/computer/computer-01-exact-window-observe.png",
            "70bbe4b57c1c5e8948c75a32eca2888154c89712c977da15ce0b04b5107cc272",
        ),
        (
            "evidence/v0.12.8/computer/computer-02-semantic-set-value.png",
            "81a5f9941805076876d070083045d8ca3b430d1e96843435d8bb70779a04a4fd",
        ),
        (
            "evidence/v0.12.8/computer/computer-03-semantic-invoke.png",
            "42fa04c44803f8b0d6d6625c253412ae3758174fdf9e2ea3e88ae71143b9b1f2",
        ),
        (
            "evidence/v0.12.8/computer/computer-04-persistent-scstream-start.png",
            "186b34f57524bf8c62eb0d80e51f4da13fb1f70ac63accc8ec06b88dfc64d7dd",
        ),
        (
            "evidence/v0.12.8/computer/computer-05-live-share-pixel-action.png",
            "fb3a3cc6a91df9bb6ff23d80cfe9721e6b382db74ac6ec0b975bd4cba0dff44f",
        ),
        (
            "evidence/v0.12.8/computer/computer-06-persistent-share-resize.png",
            "ea1ccd85eaecb600c3f721b613749c39d3172b11604dfcff8065214925c0ab86",
        ),
        (
            "evidence/v0.12.8/computer/attempts/withdrawn-532d603-windows-foreground-arm-timeout/fixture/fixture-events.ndjson",
            "9f7a994c51a5dd49702e38ad302749244c23bf5007fad55b3dc4581501515122",
        ),
        (
            "evidence/v0.12.8/computer/attempts/withdrawn-532d603-windows-foreground-arm-timeout/fixture/fixture-ready.json",
            "7e664f44a15267eaafd3f5771e89e814794d2a58f939b37e8c2f896d8c2e8c2b",
        ),
        (
            "evidence/v0.12.8/computer/attempts/withdrawn-532d603-windows-foreground-arm-timeout/fixture/fixture-state.json",
            "1e4eb43c645201c01a4154aa5f8037d25e8f449d93a54626a4ff13fa445d7b96",
        ),
        (
            "evidence/v0.12.8/computer/attempts/withdrawn-532d603-windows-foreground-arm-timeout/operator/foreground-arm-request.json",
            "8e3c365c08eb74d9fb6043e581e3b11dc1c517136ca9e093807e6852a83a0faf",
        ),
        (
            "evidence/v0.12.8/computer/attempts/withdrawn-532d603-windows-foreground-arm-timeout/steps/01-protocol-bound-helper-readiness.json",
            "50054c61bd8aed997ccaa5d3d0c3154581e983ab0490599381525c89e4948d0e",
        ),
        (
            "evidence/v0.12.8/computer/attempts/withdrawn-532d603-windows-foreground-arm-timeout/steps/02-foreground-arm-request-delivery.json",
            "9b3f793dd38c64df589bab0821f4ab35d1add16a8e2ac36e4930c405900a0a4f",
        ),
        (
            "evidence/v0.12.8/computer/attempts/withdrawn-532d603-windows-foreground-arm-timeout/summary.json",
            "a06062501fc5050cf09b49799b64ec059118b5719b92259efe6606ebdd75726d",
        ),
    ] {
        assert_eq!(
            file_sha256(path),
            expected,
            "withdrawn v0.12.8 exact-candidate evidence changed: {path}"
        );
    }

    let results: serde_json::Value = serde_json::from_str(
        &fs::read_to_string("evidence/v0.12.8/computer/helper-results.json").unwrap(),
    )
    .unwrap();
    assert_eq!(results["productVersion"], "0.12.8");
    assert_eq!(results["status"], "passed-release-candidate");
    assert_eq!(
        results["evidenceClass"],
        "exact-release-candidate-package-live-observation"
    );
    assert_eq!(results["assertions"]["passed"], 187);
    assert_eq!(results["assertions"]["failed"], 0);
    assert_eq!(results["screenshots"].as_array().unwrap().len(), 6);
    assert_eq!(
        results["package"]["checksumManifest"]["actualSha256"],
        "45d550e394a38b56aeb8a67bde3c3792d3c1728d9088d61f9576622506273a28"
    );
    assert_eq!(
        results["package"]["archive"]["sha256"],
        "651ecd9a1cb095812669884a0c8db1664d20aecd28bc65c05e50d548416dac13"
    );
    assert_eq!(results["package"]["serverVersion"], "0.12.8");
    assert_eq!(results["package"]["helperVersion"], "0.12.8");
    assert_eq!(
        results["package"]["strictCodeSignatureVerification"],
        "passed"
    );
    assert_eq!(results["harness"]["packagedHelperSpawnCount"], 1);
    for (field, path) in [
        (
            "runnerSha256",
            "evidence/v0.12.8/computer/helper-evidence-rig.mjs",
        ),
        (
            "fixtureSha256",
            "evidence/v0.12.8/computer/HelperEvidenceFixture.swift",
        ),
        (
            "systemProbeSha256",
            "evidence/v0.12.8/computer/SystemProbe.swift",
        ),
    ] {
        assert_eq!(
            results["harness"][field].as_str().unwrap(),
            file_sha256(path)
        );
    }

    fn collect_relative_files(
        root: &std::path::Path,
        current: &std::path::Path,
        out: &mut BTreeSet<String>,
    ) {
        for entry in fs::read_dir(current).unwrap().map(Result::unwrap) {
            if entry.file_type().unwrap().is_dir() {
                collect_relative_files(root, &entry.path(), out);
            } else {
                out.insert(
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    let windows_root = std::path::Path::new(
        "evidence/v0.12.8/computer/attempts/withdrawn-532d603-windows-foreground-arm-timeout",
    );
    let mut windows_files = BTreeSet::new();
    collect_relative_files(windows_root, windows_root, &mut windows_files);
    assert_eq!(
        windows_files,
        BTreeSet::from([
            "README.md".to_owned(),
            "fixture/fixture-events.ndjson".to_owned(),
            "fixture/fixture-ready.json".to_owned(),
            "fixture/fixture-state.json".to_owned(),
            "operator/foreground-arm-request.json".to_owned(),
            "steps/01-protocol-bound-helper-readiness.json".to_owned(),
            "steps/02-foreground-arm-request-delivery.json".to_owned(),
            "summary.json".to_owned(),
        ])
    );
    let summary: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(windows_root.join("summary.json")).unwrap())
            .unwrap();
    assert_eq!(summary["passed"], false);
    assert_eq!(summary["failureDetails"]["stage"], "wait-foreground-arm");
    assert_eq!(summary["steps"].as_array().unwrap().len(), 2);
    assert!(
        summary["steps"]
            .as_array()
            .unwrap()
            .iter()
            .all(|step| step["passed"] == true)
    );
    assert_eq!(
        summary["foregroundArmProof"]["fixtureAcknowledgementCount"],
        0
    );
    assert_eq!(
        summary["foregroundArmProof"]["fixtureLeftMouseDownCount"],
        0
    );
    assert_eq!(summary["foregroundArmProof"]["fixtureLeftMouseUpCount"], 0);
    assert_eq!(summary["foregroundArmProof"]["completed"], false);
    assert_eq!(summary["cleanupIssues"].as_array().unwrap().len(), 0);
    assert_eq!(summary["tokenPersistenceVerified"], true);
    assert_eq!(summary["tokenPersisted"], false);
    assert_eq!(summary["unrelatedProcessesTerminated"], false);
    assert!(
        !windows_root
            .join("operator/foreground-arm-received.json")
            .exists()
    );
    assert_eq!(
        windows_files
            .iter()
            .filter(|path| path.ends_with(".png"))
            .count(),
        0
    );

    let request: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(windows_root.join("operator/foreground-arm-request.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(request["schemaVersion"], 1);
    assert_eq!(request["status"], "action-required");
    assert_eq!(request["requestDelivered"], true);
    assert_eq!(request["buttonEnabled"], true);
    assert_eq!(request["nativeTopologyMatched"], true);
    assert_eq!(request["notificationOnly"], true);
    assert_eq!(request["acceptedAsAuthority"], false);
}

#[test]
fn withdrawn_v0_12_46_macos_receiver_probe_timeout_is_byte_exact_and_fail_closed() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.46/computer/attempts/withdrawn-a423ed2-macos-active-receiver-probe-timeout",
    );
    let expected = [
        (
            "README.md",
            2_125,
            "a8b839b97339c3266fd2c1f11a23a5aca2be66dc58fb6a9e3ff4bb65bbda08a4",
        ),
        (
            "computer-01-exact-window-observe.png",
            830_796,
            "044dc1f7b7ff84ee8dab9064be514130e77c5369b608622002ff45390f4eec4f",
        ),
        (
            "computer-02-semantic-set-value.png",
            846_550,
            "fb83b440590be23bdc3d6484a2b196ea96ee6fd9c8d50c2e594ae36c2c0b50b1",
        ),
        (
            "computer-03-semantic-invoke.png",
            846_772,
            "1c6e70bf71d745eee1a4b9aa26e671aba8ef3b4db7972bee6856c92bb8825cc2",
        ),
        (
            "computer-04-persistent-scstream-start.png",
            780_327,
            "0967cd68948cf07993922470f81c5d26e7fe9165c8f4b8381649eb684cfd13a6",
        ),
        (
            "helper-results.json",
            21_229,
            "1232095816e1de0650cbc3786d0507c7ba525dee9259b6df8f0a0c009d84ec18",
        ),
        (
            "helper-rig.log",
            9_862,
            "d7aa6664f62fda72786e1dfae4d5458260b76dc899bfca4d8a5b7ae5147ed8f0",
        ),
    ];
    let entries = fs::read_dir(attempt_root)
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| {
            let file_type = entry.file_type().unwrap();
            assert!(file_type.is_file() && !file_type.is_symlink());
            entry.file_name().to_string_lossy().into_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        entries,
        expected
            .iter()
            .map(|(name, _, _)| (*name).to_owned())
            .collect()
    );
    for (name, expected_bytes, expected_sha256) in expected {
        let path = attempt_root.join(name);
        assert_eq!(fs::metadata(&path).unwrap().len(), expected_bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(&path).unwrap())),
            expected_sha256
        );
    }

    let results: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("helper-results.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(results["schemaVersion"], 9);
    assert_eq!(results["productVersion"], "0.12.46");
    assert_eq!(results["status"], "failed-release-candidate");
    assert_eq!(results["assertions"]["passed"], 68);
    assert_eq!(results["assertions"]["failed"], 1);
    assert_eq!(results["assertions"]["total"], 69);
    assert_eq!(
        results["failureDiagnostics"]["stage"],
        "liveSharePixelAction"
    );
    assert_eq!(results["failureDiagnostics"]["actionDispatched"], false);
    assert_eq!(
        results["failureDiagnostics"]["fixtureCounters"]["after"]["clicks"],
        0
    );
    assert_eq!(
        results["failureDiagnostics"]["systemProbe"]["equality"]["sharedInputSeatActivityObserved"],
        false
    );
    assert_eq!(
        results["releaseCandidateBinding"]["artifactZipSha256"],
        "a423ed25f9b6fb2b0db8f3065c55f0a7309a4c4ff0f88fc8a5c413047dabacde"
    );
}

#[test]
fn withdrawn_v0_12_47_macos_finalizer_failure_is_byte_exact_and_not_release_evidence() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.47/computer/attempts/withdrawn-6f469e4-macos-finalizer-empty-path",
    );
    let expected = [
        (
            "README.md",
            2_232,
            "3db94bdc73d0f5d9dfed6cb949c35684b22645d7739b6af56165e4b3806d1095",
        ),
        (
            "deliberate-concurrency/computer-01-exact-window-observe.png",
            814_821,
            "dea01365f3131399dc8c6fbb84002012b293ed4b4098e70c21ca8bf9a66e843f",
        ),
        (
            "deliberate-concurrency/computer-02-semantic-set-value.png",
            830_234,
            "deac567b76c747a28788f1ba9c5156bba9cfee5b5e46e76504e6360ebab22b2b",
        ),
        (
            "deliberate-concurrency/computer-03-semantic-invoke.png",
            829_543,
            "65e28f571c7be279a8139c712cc231c8709323330fde14d90730e04a72b8e4c1",
        ),
        (
            "deliberate-concurrency/computer-04-persistent-scstream-start.png",
            722_332,
            "1ceae23f8d596507144ebc18130f27aef51db4fdf9114f5ca3eb9ca4c9e3319f",
        ),
        (
            "deliberate-concurrency/computer-05-live-share-pixel-action.png",
            722_794,
            "c3de54721a2d4aaadcb72dc482c704280bf5c890cd8c6394b31218bbb25d8ece",
        ),
        (
            "deliberate-concurrency/computer-06-persistent-share-resize.png",
            595_662,
            "eab85f168df33d1167ba0306b68b7cbfaa8a0d9d7becf9717344ae2ae8e4aed9",
        ),
        (
            "deliberate-concurrency/helper-results.json",
            136_239,
            "53b77a249151c6136dbdb9cbcaa49170b1cef1e62b059052d8943f94693bb1b8",
        ),
        (
            "deliberate-concurrency/helper-rig.log",
            45_713,
            "8e7f120601ef6e1ed3e430fc054d7c9e448ac3e3d731bd411019b2cd2bbd5651",
        ),
        (
            "deliberate-concurrency/operator/macos-app-share-concurrency-handoff-complete.json",
            671,
            "5d54a0d9b2003a57e135211d630cde7c4b0f3ef842d52d79339b58b18284751b",
        ),
        (
            "deliberate-concurrency/operator/macos-app-share-concurrency-handoff-request.json",
            797,
            "66a9283a3baf9b4463e06ed4edcc538f5ffc3f1ccba7632d5864190b8a28f609",
        ),
        (
            "deliberate-concurrency/operator/macos-app-share-concurrency-handoff-start.json",
            442,
            "509b1c0351a2b2d364098e9edc060bb81c2d710fed8781a16a8173c35d117617",
        ),
        (
            "quiet/computer-01-exact-window-observe.png",
            809_399,
            "363f12a848f7dbf0fe86a7f60fc9617a764141214c75ec6036ab7212ab67ebcf",
        ),
        (
            "quiet/computer-02-semantic-set-value.png",
            825_053,
            "f9469bb11f385500b7926bb1c98479ca06300f5e39d219dd43fb2b4e3f05a432",
        ),
        (
            "quiet/computer-03-semantic-invoke.png",
            825_351,
            "75cde88baafdc7b9a4b6816ea6a1dda7533bcfdbd5be52d328b332dd293697a8",
        ),
        (
            "quiet/computer-04-persistent-scstream-start.png",
            715_781,
            "db31c8df7ebb0aa463b1fc1bdf9a17268c4c45430c6081e60b658fb223f7cd99",
        ),
        (
            "quiet/computer-05-live-share-pixel-action.png",
            716_365,
            "0880b641b3b5b6577c8dc55f002f86ea720e8a2e3ffc66709c921cfa3d217e75",
        ),
        (
            "quiet/computer-06-persistent-share-resize.png",
            588_870,
            "6cc1fc9a0ae8066b1b8cd1fed9317f6fba74756aedc2a63d2af62ba3ce66358b",
        ),
        (
            "quiet/helper-results.json",
            125_416,
            "2b638da2ec177f803e8726fb37c1f5ae344ed175207d75426bf36958abe7726d",
        ),
        (
            "quiet/helper-rig.log",
            40_471,
            "8a4e89dca339d844b7683ce237641b88a54fb41d71c96a39eaf26ee4afcbe2cd",
        ),
    ];

    fn collect_files(
        root: &std::path::Path,
        directory: &std::path::Path,
        files: &mut BTreeSet<String>,
    ) {
        for entry in fs::read_dir(directory).unwrap().map(Result::unwrap) {
            let file_type = entry.file_type().unwrap();
            assert!(!file_type.is_symlink());
            if file_type.is_dir() {
                collect_files(root, &entry.path(), files);
            } else {
                assert!(file_type.is_file());
                files.insert(
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    let mut actual_files = BTreeSet::new();
    collect_files(attempt_root, attempt_root, &mut actual_files);
    assert_eq!(
        actual_files,
        expected
            .iter()
            .map(|(name, _, _)| (*name).to_owned())
            .collect()
    );
    for (name, expected_bytes, expected_sha256) in expected {
        let path = attempt_root.join(name);
        assert_eq!(fs::metadata(&path).unwrap().len(), expected_bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            expected_sha256
        );
    }

    let quiet: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
    )
    .unwrap();
    let deliberate: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("deliberate-concurrency/helper-results.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(quiet["productVersion"], "0.12.47");
    assert_eq!(quiet["status"], "passed-release-candidate");
    assert_eq!(quiet["assertions"]["passed"], 207);
    assert_eq!(deliberate["productVersion"], "0.12.47");
    assert_eq!(deliberate["status"], "passed-release-candidate");
    assert_eq!(deliberate["assertions"]["passed"], 231);
    assert_eq!(deliberate["appShareHandoff"]["actionDispatched"], true);
    assert_eq!(
        deliberate["releaseCandidateBinding"]["artifactZipSha256"],
        "6f469e4e0f28c754c292f3a7a5e64671daba45ca3eed8212a1f1558b9d8ee64a"
    );
    let readme = fs::read_to_string(attempt_root.join("README.md")).unwrap();
    assert!(readme.contains("It is not release evidence."));
    assert!(readme.contains("aggregate output directory path is invalid."));
    assert!(readme.contains("The finalizer was not retried."));
}

#[test]
fn withdrawn_v0_12_50_macos_scratch_path_failure_is_sanitized_and_fail_closed() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.50/computer/attempts/withdrawn-e5530ce-macos-coordinator-scratch-path",
    );
    let expected = [
        (
            "README.md",
            2_105,
            "26e96d48245559617c2c3e0294a2626ce797e5d1c1441685c3ffb4697716aef9",
        ),
        (
            "deliberate-concurrency/helper-results.json",
            5_865,
            "8d3e5a37ced360fc148a60556f0ebe9b7e7a1c631b342e69de76ee6c0efe1447",
        ),
        (
            "deliberate-concurrency/helper-rig.log",
            993,
            "d5329cff9b1ebe8f4ff9df852c37c38858c95b9c2692e6922809cedad4a41d47",
        ),
        (
            "quiet/computer-01-exact-window-observe.png",
            831_513,
            "d9cc5e6e26c05149318b6217e2de982c69b5e604c297ce53698b20b53007a3bb",
        ),
        (
            "quiet/computer-02-semantic-set-value.png",
            846_988,
            "590f6e63838775f02b09af31ebdda3acfcd4aab07bc37f3e90259495b4220463",
        ),
        (
            "quiet/computer-03-semantic-invoke.png",
            847_977,
            "7dfd30551a899897336ac9d97daa167bef05582075a84c2b28a9d6b685e5eec0",
        ),
        (
            "quiet/computer-04-persistent-scstream-start.png",
            781_242,
            "2ac2ec43a1f22b563b546efe3f4a7de0d413dc939d0e947c2680677a71ede52b",
        ),
        (
            "quiet/computer-05-live-share-pixel-action.png",
            781_295,
            "43c55f3106833f11cccf906cb389f6d141363d89b820cb5a4459977a1840146d",
        ),
        (
            "quiet/computer-06-persistent-share-resize.png",
            709_838,
            "8ecf2c8c104792ae268b0a24219b540494117bc4532a901b593ce7f5236a9ab0",
        ),
        (
            "quiet/helper-results.json",
            125_449,
            "a5428392121903ab3fad2c87f80674232a332444b0b9217d139c92092316d3a0",
        ),
        (
            "quiet/helper-rig.log",
            36_840,
            "65760d588eb58b5d84193490b2f722285ce2bfffa3bd12cfdedbdff40dc1a2d5",
        ),
    ];

    fn collect_files(
        root: &std::path::Path,
        directory: &std::path::Path,
        files: &mut BTreeSet<String>,
    ) {
        for entry in fs::read_dir(directory).unwrap().map(Result::unwrap) {
            let file_type = entry.file_type().unwrap();
            assert!(!file_type.is_symlink());
            if file_type.is_dir() {
                collect_files(root, &entry.path(), files);
            } else {
                assert!(file_type.is_file());
                files.insert(
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    let mut actual_files = BTreeSet::new();
    collect_files(attempt_root, attempt_root, &mut actual_files);
    assert_eq!(
        actual_files,
        expected
            .iter()
            .map(|(name, _, _)| (*name).to_owned())
            .collect()
    );
    for (name, expected_bytes, expected_sha256) in expected {
        let path = attempt_root.join(name);
        assert_eq!(fs::metadata(&path).unwrap().len(), expected_bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            expected_sha256
        );
    }

    let quiet: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
    )
    .unwrap();
    let deliberate: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("deliberate-concurrency/helper-results.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(quiet["productVersion"], "0.12.50");
    assert_eq!(quiet["status"], "passed-release-candidate");
    assert_eq!(quiet["assertions"]["passed"], 207);
    assert_eq!(quiet["assertions"]["failed"], 0);
    assert_eq!(deliberate["productVersion"], "0.12.50");
    assert_eq!(deliberate["status"], "failed-release-candidate");
    assert_eq!(deliberate["assertions"]["passed"], 7);
    assert_eq!(deliberate["assertions"]["failed"], 0);
    assert_eq!(deliberate["fixture"], serde_json::Value::Null);
    assert_eq!(
        deliberate["fatal"],
        "ENOENT: no such file or directory, access '<redacted-owner-private-scratch-parent>'"
    );
    assert_eq!(
        deliberate["releaseCandidateBinding"]["artifactZipSha256"],
        "e5530cea34ef89733dbb4088baca019fe6c0683621f945f12d962ef0bdf6a407"
    );

    let retained_text = [
        fs::read_to_string(attempt_root.join("README.md")).unwrap(),
        fs::read_to_string(attempt_root.join("deliberate-concurrency/helper-results.json"))
            .unwrap(),
        fs::read_to_string(attempt_root.join("deliberate-concurrency/helper-rig.log")).unwrap(),
    ]
    .join("\n");
    assert!(retained_text.contains("It is not release evidence."));
    assert!(retained_text.contains("The deliberate lane was not\nretried"));
    assert!(retained_text.contains("<redacted-owner-private-scratch-parent>"));
    assert!(!retained_text.contains("/private/tmp/"));
}

#[test]
fn withdrawn_v0_12_51_windows_token_prompt_is_sanitized_and_fail_closed() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.51/computer/attempts/withdrawn-fce8aa47-windows-trust-token-prompt",
    );
    let expected = [
        (
            "README.md",
            1_668,
            "82621893d5c3df27443b6f244fa8e119a456c94a8c6b640b0a01e9427f1793b7",
        ),
        (
            "failure.json",
            751,
            "77b725c4d2c916920d3a804754462507807694d509d7e69c51a5cdc93e7c5dc7",
        ),
    ];
    let actual = fs::read_dir(attempt_root)
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| {
            let file_type = entry.file_type().unwrap();
            assert!(file_type.is_file());
            assert!(!file_type.is_symlink());
            entry.file_name().to_string_lossy().into_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|(name, _, _)| (*name).to_owned())
            .collect()
    );
    for (name, expected_bytes, expected_sha256) in expected {
        let path = attempt_root.join(name);
        assert_eq!(fs::metadata(&path).unwrap().len(), expected_bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            expected_sha256
        );
    }

    let failure: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(attempt_root.join("failure.json")).unwrap())
            .unwrap();
    assert_eq!(failure["outcome"], "withdrawn");
    assert_eq!(
        failure["failureCode"],
        "missing-gh-token-noninteractive-prompt"
    );
    assert_eq!(failure["trustInvocationCount"], 1);
    assert_eq!(failure["sourceCloneCompleted"], true);
    assert_eq!(failure["candidatePayloadDirectoryEmpty"], true);
    assert_eq!(failure["candidateArtifactDownloaded"], false);
    assert_eq!(failure["candidateExecutableStarted"], false);
    assert_eq!(failure["stockChromeStarted"], false);
    assert_eq!(failure["computerUseStarted"], false);
    assert_eq!(failure["uiActionCount"], 0);
    assert_eq!(failure["taskOwnedProcessesCleaned"], true);
    assert_eq!(failure["listener17373PresentAfterCleanup"], false);
    assert_eq!(failure["retryProhibited"], true);

    let retained_text = fs::read_to_string(attempt_root.join("README.md")).unwrap();
    assert!(retained_text.contains("It is not release evidence."));
    assert!(retained_text.contains("It was not retried."));
    for forbidden in ["C:\\Users\\", "/Users/", "GH_TOKEN=", "ghp_", "github_pat_"] {
        assert!(!retained_text.contains(forbidden));
    }
}

#[test]
fn withdrawn_v0_12_54_local_move_receiver_race_is_byte_exact_and_non_release() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.54/computer/attempts/withdrawn-local-macos-cancellation-move-key-window-race",
    );
    let expected = [
        (
            "README.md",
            1_928,
            "b1d88deafdd48d62af7b0221cb12ffe3f19c5268748b97cde80139c8de23fbbd",
        ),
        (
            "quiet/computer-01-exact-window-observe.png",
            836_035,
            "97415e263bef36e66e3b5da8eb276913779bea91f47c888b0bb05057058e34a2",
        ),
        (
            "quiet/computer-02-semantic-set-value.png",
            850_766,
            "07272b377fcc0bd175b42de49add0bf0225213f143116eaf7caaa474aec01bde",
        ),
        (
            "quiet/computer-03-semantic-invoke.png",
            853_186,
            "6653135447fd89f941aa94192d5de2864c0fcef7d218d480bfbbdc1551028b26",
        ),
        (
            "quiet/computer-04-persistent-scstream-start.png",
            780_338,
            "91c241916682509b12fda2aaf6f28aeec1c461d51cfdc7fdeedddf5dca57a652",
        ),
        (
            "quiet/computer-05-live-share-pixel-action.png",
            780_321,
            "65bcf3c9ae28214198e764e79f1bdb740b7d306cf4e91090b93cf77ac5a02c65",
        ),
        (
            "quiet/computer-06-persistent-share-resize.png",
            706_290,
            "db0f870c0073ec4b39a639e13102abdde7e2015acb3fa0d3d401692ffe3513aa",
        ),
        (
            "quiet/helper-results.json",
            44_381,
            "025944be76d828071b1d04fa557a70f0b7136e996d2f5cdd401da6e15e731bc1",
        ),
        (
            "quiet/helper-rig.log",
            27_499,
            "4cc993306f9d4139a55822b9d590454b30f9849a39c1966e362d3d7b26e30b44",
        ),
    ];
    assert_eq!(
        walk_files(attempt_root),
        expected
            .iter()
            .map(|(name, _, _)| (*name).to_owned())
            .collect()
    );
    for (name, bytes, sha256) in expected {
        let path = attempt_root.join(name);
        assert_eq!(fs::metadata(&path).unwrap().len(), bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            sha256
        );
    }
    let result: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(result["productVersion"], "0.12.54");
    assert_eq!(result["status"], "failed-release-candidate");
    assert_eq!(result["assertions"]["passed"], 158);
    assert_eq!(result["failureDiagnostics"]["actionDispatched"], false);
    assert_eq!(
        result["failureDiagnostics"]["fixtureCounters"]["before"]["moveEvents"],
        result["failureDiagnostics"]["fixtureCounters"]["after"]["moveEvents"]
    );
    let retained = [
        fs::read_to_string(attempt_root.join("README.md")).unwrap(),
        fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
        fs::read_to_string(attempt_root.join("quiet/helper-rig.log")).unwrap(),
    ]
    .join("\n");
    assert!(retained.contains("terminal local negative evidence, not release evidence"));
    assert!(retained.contains("No GitHub release-candidate workflow or artifact existed"));
    for forbidden in [
        "/private/tmp/",
        "/Users/",
        "GH_TOKEN=",
        "ghp_",
        "github_pat_",
    ] {
        assert!(!retained.contains(forbidden));
    }
}

#[test]
fn withdrawn_v0_12_55_local_move_observer_race_is_byte_exact_and_non_release() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.55/computer/attempts/withdrawn-local-macos-move-observer-first-responder-race",
    );
    let expected = [
        (
            "README.md",
            1_835,
            "f27d9d1bae1bfadcaf508ef6041f2347a05d8192af01c3a59cf9659fe2e31819",
        ),
        (
            "quiet/computer-01-exact-window-observe.png",
            836_733,
            "66be1351aca9cba75f28f07d20dad6419aec523fb124bf0db531dee15b9c5a2c",
        ),
        (
            "quiet/computer-02-semantic-set-value.png",
            851_564,
            "5cf955770ce0f166bde8727f42f51b2874f7f4dbd4b161d24c98b54dee4dc6c6",
        ),
        (
            "quiet/computer-03-semantic-invoke.png",
            853_800,
            "2f4fc480b1b0f46c35486496cdfb34e8a7f325d039c8e0d5e18d98d0fcc13dab",
        ),
        (
            "quiet/computer-04-persistent-scstream-start.png",
            677_793,
            "e9ce455841eb2bf6b79da30a762a69c58d27d6e4dc90891fdcf1d1ac1c6d04d4",
        ),
        (
            "quiet/computer-05-live-share-pixel-action.png",
            678_165,
            "7427ce4579eec40ab1573715fb424666991ccf914b6d5138530ba95bddf5976f",
        ),
        (
            "quiet/computer-06-persistent-share-resize.png",
            543_178,
            "432f03adff7d01930aeca442cfad36e9487d89cce97f0bb71d84da0612aa9071",
        ),
        (
            "quiet/helper-results.json",
            44_381,
            "df0fc1b4baecc22a418c704cf456100aed5ca5be06389686415e95f039be4fa1",
        ),
        (
            "quiet/helper-rig.log",
            27_499,
            "51499f93fb6e7344075bf40ec4088cd8f3cf6069c75f58af36f09eb11e26bc37",
        ),
    ];
    assert_eq!(
        walk_files(attempt_root),
        expected
            .iter()
            .map(|(name, _, _)| (*name).to_owned())
            .collect()
    );
    for (name, bytes, sha256) in expected {
        let path = attempt_root.join(name);
        assert_eq!(fs::metadata(&path).unwrap().len(), bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            sha256
        );
    }
    let result: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(result["productVersion"], "0.12.55");
    assert_eq!(result["status"], "failed-release-candidate");
    assert_eq!(result["assertions"]["passed"], 158);
    assert_eq!(result["failureDiagnostics"]["actionDispatched"], false);
    assert_eq!(
        result["failureDiagnostics"]["fixtureCounters"]["before"]["moveEvents"],
        result["failureDiagnostics"]["fixtureCounters"]["after"]["moveEvents"]
    );
    let retained = [
        fs::read_to_string(attempt_root.join("README.md")).unwrap(),
        fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
        fs::read_to_string(attempt_root.join("quiet/helper-rig.log")).unwrap(),
    ]
    .join("\n");
    let normalized_retained = retained.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(retained.contains("terminal local negative evidence, not release evidence"));
    assert!(normalized_retained.contains("No GitHub candidate workflow or artifact existed"));
    for forbidden in [
        "/private/tmp/",
        "/Users/",
        "GH_TOKEN=",
        "ghp_",
        "github_pat_",
    ] {
        assert!(!retained.contains(forbidden));
    }
}

#[test]
fn withdrawn_v0_12_56_local_window_move_observer_race_is_byte_exact_and_non_release() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.56/computer/attempts/withdrawn-local-macos-window-move-observer-race",
    );
    let expected = [
        (
            "README.md",
            1_941,
            "72f7c8fd720b9543c8f9dcfbd8d7f7a6acc97d9f690e6fbc51072a4d9c0047d1",
        ),
        (
            "quiet/computer-01-exact-window-observe.png",
            836_584,
            "e0b20f58220a3a62f41dc21fb512ba77eb1be26d9c631487790b2b1c82ecedc8",
        ),
        (
            "quiet/computer-02-semantic-set-value.png",
            852_491,
            "594be56d521e99db1edffb7b906ba63dcfc8727a58de4c40122566a553c0f632",
        ),
        (
            "quiet/computer-03-semantic-invoke.png",
            854_025,
            "117f3416697b83ff0829b9f6d2453ce92358665a1b6c0941ddc31da242392851",
        ),
        (
            "quiet/computer-04-persistent-scstream-start.png",
            678_341,
            "a3ca4c2599653c1beded25ea5bd5a8cd0142cda192a2b133383c7128fc5499fc",
        ),
        (
            "quiet/computer-05-live-share-pixel-action.png",
            782_872,
            "55a853ae9c8d9b3a00335285c95b10531f6008f61e19bcc09514c8c2e3d92456",
        ),
        (
            "quiet/computer-06-persistent-share-resize.png",
            706_808,
            "d9c00fa0da16d5f0ec995bfbd3bf72ec3f7a1f5e6114840c8702ee8cb8b622ae",
        ),
        (
            "quiet/helper-results.json",
            44_381,
            "2840c4254371a3e5278809b65bf8c5ac96de9cbb0a2f1968618cc2a2ac978f73",
        ),
        (
            "quiet/helper-rig.log",
            27_499,
            "f5bbc7919219e81b247a5ce669080a453fb256d67fcfa31d555aaf4648f237a2",
        ),
    ];
    assert_eq!(
        walk_files(attempt_root),
        expected
            .iter()
            .map(|(name, _, _)| (*name).to_owned())
            .collect()
    );
    for (name, bytes, sha256) in expected {
        let path = attempt_root.join(name);
        assert_eq!(fs::metadata(&path).unwrap().len(), bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            sha256
        );
    }
    let result: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(result["productVersion"], "0.12.56");
    assert_eq!(result["status"], "failed-release-candidate");
    assert_eq!(result["assertions"]["passed"], 158);
    assert_eq!(result["failureDiagnostics"]["actionDispatched"], false);
    assert_eq!(
        result["failureDiagnostics"]["fixtureCounters"]["before"]["moveEvents"],
        result["failureDiagnostics"]["fixtureCounters"]["after"]["moveEvents"]
    );
    let retained = [
        fs::read_to_string(attempt_root.join("README.md")).unwrap(),
        fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
        fs::read_to_string(attempt_root.join("quiet/helper-rig.log")).unwrap(),
    ]
    .join("\n");
    let normalized_retained = retained.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(retained.contains("terminal local negative evidence, not release evidence"));
    assert!(normalized_retained.contains("No GitHub candidate workflow or artifact existed"));
    assert!(normalized_retained.contains("package then passed 207 of 207 checks"));
    for forbidden in [
        "/private/tmp/",
        "/Users/",
        "GH_TOKEN=",
        "ghp_",
        "github_pat_",
    ] {
        assert!(!retained.contains(forbidden));
    }
}

#[test]
fn withdrawn_v0_12_57_local_cancellation_stale_frame_is_byte_exact_and_non_release() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.57/computer/attempts/withdrawn-local-macos-cancellation-stale-frame-race",
    );
    let expected = [
        (
            "README.md",
            1_893,
            "eeca72a109fa8262f4aef858f091d8ddcc1ab9da2874e42b577907cb4eb28fd3",
        ),
        (
            "quiet/computer-01-exact-window-observe.png",
            835_937,
            "1db042e1a4dfa2f284bef24ad909041835b1229b993162dd154d45627f8a8c92",
        ),
        (
            "quiet/computer-02-semantic-set-value.png",
            851_004,
            "be15e556aca23a3e4c99f77f0eb148df546cd39ae3c44497a80e199408811186",
        ),
        (
            "quiet/computer-03-semantic-invoke.png",
            853_326,
            "5076c352db022489090292448a9a68647be84a05154f6c812a5b84e8a4bd5f19",
        ),
        (
            "quiet/computer-04-persistent-scstream-start.png",
            780_225,
            "58ea6fa35bdcf7dd7ce847ec82100396165a3f5c763e5cb7a602e2d8380db6b0",
        ),
        (
            "quiet/computer-05-live-share-pixel-action.png",
            781_246,
            "acc5e30c7efdca281b74b4dfd95ad4e6ff50433c807d05e591e226fd376a84aa",
        ),
        (
            "quiet/computer-06-persistent-share-resize.png",
            542_300,
            "92eb5950795fba51bf54c123859e7be58d10565344ad66fc9c1f7af0ccc80e96",
        ),
        (
            "quiet/helper-results.json",
            44_381,
            "226da6e4765f7099f69fac4f764411eb681ee42c89d26e6ca9cb2382ef643067",
        ),
        (
            "quiet/helper-rig.log",
            27_499,
            "6300b0131218cd19ddfe197980917e1dcaa3b11bb049f74c1cd66f4344d8edc5",
        ),
    ];
    assert_eq!(
        walk_files(attempt_root),
        expected
            .iter()
            .map(|(name, _, _)| (*name).to_owned())
            .collect()
    );
    for (name, bytes, sha256) in expected {
        let path = attempt_root.join(name);
        assert_eq!(fs::metadata(&path).unwrap().len(), bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            sha256
        );
    }
    let result: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(result["productVersion"], "0.12.57");
    assert_eq!(result["status"], "failed-release-candidate");
    assert_eq!(result["assertions"]["passed"], 158);
    assert_eq!(result["failureDiagnostics"]["actionDispatched"], false);
    assert_eq!(
        result["failureDiagnostics"]["fixtureCounters"]["before"]["moveEvents"],
        result["failureDiagnostics"]["fixtureCounters"]["after"]["moveEvents"]
    );
    let retained = [
        fs::read_to_string(attempt_root.join("README.md")).unwrap(),
        fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
        fs::read_to_string(attempt_root.join("quiet/helper-rig.log")).unwrap(),
    ]
    .join("\n");
    let normalized_retained = retained.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(retained.contains("terminal local negative evidence, not release evidence"));
    assert!(normalized_retained.contains("No GitHub candidate workflow or artifact existed"));
    assert!(normalized_retained.contains("exact HTTP 409 `COMPUTER_STALE_FRAME`"));
    for forbidden in [
        "/private/tmp/",
        "/Users/",
        "GH_TOKEN=",
        "ghp_",
        "github_pat_",
    ] {
        assert!(!retained.contains(forbidden));
    }
}

#[test]
fn withdrawn_v0_12_53_macos_move_delivery_timeout_is_byte_exact_and_fail_closed() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.53/computer/attempts/withdrawn-f5e7c85b-macos-cancellation-move-delivery-timeout",
    );
    let expected = [
        (
            "README.md",
            2_237,
            "102849925e8675dcd59f36d267eb5ebee68f97162cef396102209eea44edde39",
        ),
        (
            "quiet/computer-01-exact-window-observe.png",
            836_883,
            "eee31e3f2fe51f1e54e43de9da64774dc211adbbdb260602780db049e5e633eb",
        ),
        (
            "quiet/computer-02-semantic-set-value.png",
            852_493,
            "6f3a71b11e3ded0de99cad4fb53524b5780a7d097b6e1a148d7e1a1860b615b7",
        ),
        (
            "quiet/computer-03-semantic-invoke.png",
            854_563,
            "016f14dafa98fcc9a48a362686be31e0aa863cf481508dc8be61660a7d337d50",
        ),
        (
            "quiet/computer-04-persistent-scstream-start.png",
            781_890,
            "f9d2ef49bf1e878e64cef4abc8a9231d6005459ba63cbd9badb0e5f5c1eb2396",
        ),
        (
            "quiet/computer-05-live-share-pixel-action.png",
            782_740,
            "dba4dc78091fe4ae7086d381673cfe1f715c1464bcf74aff1e734c011512f54e",
        ),
        (
            "quiet/computer-06-persistent-share-resize.png",
            706_608,
            "0e425ae4fc26bfa42c0b224065dca96ef33bf3a1c6fc40588c0fd819543377f3",
        ),
        (
            "quiet/helper-results.json",
            44_400,
            "88e4e64a4a055fa21ff3f32711e243f9c740f20c8a7fe2730847f110fa68679c",
        ),
        (
            "quiet/helper-rig.log",
            27_499,
            "001aeee1fbf1a8d96dd992969a5191b3c4fbc10f4c199612fa484aef55b110ed",
        ),
    ];
    let actual = walk_files(attempt_root);
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|(name, _, _)| (*name).to_owned())
            .collect()
    );
    for (name, expected_bytes, expected_sha256) in expected {
        let path = attempt_root.join(name);
        assert_eq!(fs::metadata(&path).unwrap().len(), expected_bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            expected_sha256
        );
    }

    let result: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(result["productVersion"], "0.12.53");
    assert_eq!(result["status"], "failed-release-candidate");
    assert_eq!(result["assertions"]["passed"], 158);
    assert_eq!(result["assertions"]["failed"], 0);
    assert_eq!(
        result["fatal"],
        "fixture target-routed cancellation move dispatch timed out"
    );
    assert_eq!(result["failureDiagnostics"]["actionDispatched"], false);
    assert_eq!(
        result["failureDiagnostics"]["fixtureCounters"]["before"]["moveEvents"],
        result["failureDiagnostics"]["fixtureCounters"]["after"]["moveEvents"]
    );
    assert_eq!(
        result["failureDiagnostics"]["targetSiblingReceiver"]["expectationMet"],
        true
    );
    assert_eq!(result["screenshots"].as_array().unwrap().len(), 6);

    let retained_text = [
        fs::read_to_string(attempt_root.join("README.md")).unwrap(),
        fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
        fs::read_to_string(attempt_root.join("quiet/helper-rig.log")).unwrap(),
    ]
    .join("\n");
    assert!(retained_text.contains("This is terminal negative evidence, not release evidence."));
    assert!(retained_text.contains("The v0.12.53 attempt was not retried."));
    for forbidden in [
        "/private/tmp/",
        "/Users/",
        "GH_TOKEN=",
        "ghp_",
        "github_pat_",
    ] {
        assert!(!retained_text.contains(forbidden));
    }
}

#[test]
fn withdrawn_v0_12_52_macos_move_delivery_timeout_is_byte_exact_and_fail_closed() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.52/computer/attempts/withdrawn-e4461d4b-macos-cancellation-move-delivery-timeout",
    );
    let expected = [
        (
            "README.md",
            2_250,
            "be86e2cc3d85cffa27bc81604809e135d9fe24b3e4649bbc61541905d724287c",
        ),
        (
            "quiet/computer-01-exact-window-observe.png",
            836_486,
            "f2a6e8c8d56b625a66eb9ca23e00e520a6842a89f2bb91f71d92a353e2282a47",
        ),
        (
            "quiet/computer-02-semantic-set-value.png",
            851_602,
            "1d323b468515e286a9b9f5e235d6696ac69b48ae84b537dee162a16c71c8c340",
        ),
        (
            "quiet/computer-03-semantic-invoke.png",
            853_884,
            "35f02a06b42397e2114e5d5a445aa7d6e88631ca25f2945f41d2ab9792294a51",
        ),
        (
            "quiet/computer-04-persistent-scstream-start.png",
            781_561,
            "70423d07d90b692294d585c20550ed6fa63f1c03319f25e7b8bc567c15de129a",
        ),
        (
            "quiet/computer-05-live-share-pixel-action.png",
            782_453,
            "3c1ca3848ed74e87ea64616e0b0b050647981ab26debe8ea9c1f6ce95dd409d4",
        ),
        (
            "quiet/computer-06-persistent-share-resize.png",
            706_404,
            "677ae676d7e72cf537b9d71221725b3f6965713be8e7d2070f9a0a0d16e28080",
        ),
        (
            "quiet/helper-results.json",
            44_400,
            "dce9c4e8caf73ab4aea88540f62830aeea9136d1078189770a49a1f1921b1ec3",
        ),
        (
            "quiet/helper-rig.log",
            27_499,
            "a4140ec188d7f19f193d8f73606e0c95b4ff6956061a187cb150306b072ab051",
        ),
    ];
    let actual = walk_files(attempt_root);
    assert_eq!(
        actual,
        expected
            .iter()
            .map(|(name, _, _)| (*name).to_owned())
            .collect()
    );
    for (name, expected_bytes, expected_sha256) in expected {
        let path = attempt_root.join(name);
        assert_eq!(fs::metadata(&path).unwrap().len(), expected_bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            expected_sha256
        );
    }

    let result: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(result["productVersion"], "0.12.52");
    assert_eq!(result["status"], "failed-release-candidate");
    assert_eq!(result["assertions"]["passed"], 158);
    assert_eq!(result["assertions"]["failed"], 0);
    assert_eq!(
        result["fatal"],
        "fixture target-routed cancellation move dispatch timed out"
    );
    assert_eq!(result["failureDiagnostics"]["actionDispatched"], false);
    assert_eq!(
        result["failureDiagnostics"]["fixtureCounters"]["before"]["moveEvents"],
        result["failureDiagnostics"]["fixtureCounters"]["after"]["moveEvents"]
    );
    assert_eq!(
        result["failureDiagnostics"]["targetSiblingReceiver"]["expectationMet"],
        true
    );
    assert_eq!(result["screenshots"].as_array().unwrap().len(), 6);

    let retained_text = [
        fs::read_to_string(attempt_root.join("README.md")).unwrap(),
        fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
        fs::read_to_string(attempt_root.join("quiet/helper-rig.log")).unwrap(),
    ]
    .join("\n");
    assert!(retained_text.contains("It is not release evidence."));
    assert!(retained_text.contains("It was not retried."));
    for forbidden in [
        "/private/tmp/",
        "/Users/",
        "GH_TOKEN=",
        "ghp_",
        "github_pat_",
    ] {
        assert!(!retained_text.contains(forbidden));
    }
}

#[test]
fn withdrawn_v0_12_48_macos_share_authority_failure_is_byte_exact_and_fail_closed() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.48/computer/attempts/withdrawn-8b98da5-macos-share-authority-age-bound",
    );
    let expected = [
        (
            "README.md",
            2_358,
            "4863cc20f17d07fcca39118673811bf3cfa39affccf6c275973211ded7dfdb49",
        ),
        (
            "deliberate-concurrency/computer-01-exact-window-observe.png",
            835_300,
            "2a518d45fe643a94642e2f665dbedf637c77878dd4e491f100845bf0549dd874",
        ),
        (
            "deliberate-concurrency/computer-02-semantic-set-value.png",
            850_927,
            "3797fdc01462a709896be1362406c331d9e7a753ce348b11b52ce2f8572b11fd",
        ),
        (
            "deliberate-concurrency/computer-03-semantic-invoke.png",
            851_237,
            "c4c49a644422b463ace52b597f937c1daa315c236a35a1e2364068eeec806704",
        ),
        (
            "deliberate-concurrency/computer-04-persistent-scstream-start.png",
            786_233,
            "70a2c195694b772935bd978330521c7521f8f5c9cac7ffae086ac83add48e2af",
        ),
        (
            "deliberate-concurrency/computer-05-live-share-pixel-action.png",
            786_990,
            "12faa73b01fc15526b9c7b6cdc08c171b82c83d1f9c5aa663d9b18b949b4faa8",
        ),
        (
            "deliberate-concurrency/computer-06-persistent-share-resize.png",
            714_440,
            "5d0b317b4612dbcaf9cf2e4900e56053d96aa664a062f47194da8b3d4e8fafcd",
        ),
        (
            "deliberate-concurrency/helper-results.json",
            30_903,
            "4be42d9c38d0638bf84c75bc191f0b2b62cd33dc6e4b9de57ebb95b8dc76bd27",
        ),
        (
            "deliberate-concurrency/helper-rig.log",
            17_726,
            "99094dc7a47a5461a96f744a8ca30515792281ba0295a0606cb2ea06b07345e4",
        ),
        (
            "deliberate-concurrency/operator/macos-app-share-concurrency-handoff-request.json",
            799,
            "35caae137dfe0c26b76c1c9b747d0ce3c56ee5d73ef579d3949986b98fdb4c61",
        ),
        (
            "deliberate-concurrency/operator/macos-app-share-concurrency-handoff-start.json",
            443,
            "d88a80b9d6884d0f1e0641b2af7cb7c10b6ebb3cf8bc15e91034d5314eda352a",
        ),
        (
            "quiet/computer-01-exact-window-observe.png",
            830_993,
            "d83b9abc38aa0086e69f942e19f1a685aba9a04a2ef5b35437a855b4a988d606",
        ),
        (
            "quiet/computer-02-semantic-set-value.png",
            847_111,
            "69fef02ab44fbe75e8a4af9139fa5f9f606762d1797395e90f5297d8132951bd",
        ),
        (
            "quiet/computer-03-semantic-invoke.png",
            847_291,
            "c576e8ee668c7c08593a450377499feebfe2722298fd82054503672e3de02b08",
        ),
        (
            "quiet/computer-04-persistent-scstream-start.png",
            780_707,
            "370e7841fd07e436bacd795292bd36f147ab47b0e7eca59050cb96794489e685",
        ),
        (
            "quiet/computer-05-live-share-pixel-action.png",
            781_151,
            "3eec7e0b26b58884194994ea100d013e83486f3ad528f41547a5afb04dc54972",
        ),
        (
            "quiet/computer-06-persistent-share-resize.png",
            710_072,
            "336151dc3b523d96dc66386b35ed0f2ad8192863c2359a8385793a8487ba5b1d",
        ),
        (
            "quiet/helper-results.json",
            125_440,
            "6cdc34bf8f3aeba53426511f1fedf870742de6869fde9d4a620a2b9e7af0eb74",
        ),
        (
            "quiet/helper-rig.log",
            36_835,
            "840bbb73b123d0f1c4ddcb0ababdd9ee8cf8a625a027bf802210b9c9cf1f2ff3",
        ),
    ];

    fn collect_files(
        root: &std::path::Path,
        directory: &std::path::Path,
        files: &mut BTreeSet<String>,
    ) {
        for entry in fs::read_dir(directory).unwrap().map(Result::unwrap) {
            let file_type = entry.file_type().unwrap();
            assert!(!file_type.is_symlink());
            if file_type.is_dir() {
                collect_files(root, &entry.path(), files);
            } else {
                assert!(file_type.is_file());
                files.insert(
                    entry
                        .path()
                        .strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }

    let mut actual_files = BTreeSet::new();
    collect_files(attempt_root, attempt_root, &mut actual_files);
    assert_eq!(
        actual_files,
        expected
            .iter()
            .map(|(name, _, _)| (*name).to_owned())
            .collect()
    );
    for (name, expected_bytes, expected_sha256) in expected {
        let path = attempt_root.join(name);
        assert_eq!(fs::metadata(&path).unwrap().len(), expected_bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            expected_sha256
        );
    }

    let quiet: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("quiet/helper-results.json")).unwrap(),
    )
    .unwrap();
    let deliberate: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("deliberate-concurrency/helper-results.json"))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(quiet["productVersion"], "0.12.48");
    assert_eq!(quiet["status"], "passed-release-candidate");
    assert_eq!(quiet["assertions"]["passed"], 207);
    assert_eq!(deliberate["productVersion"], "0.12.48");
    assert_eq!(deliberate["status"], "failed-release-candidate");
    assert_eq!(deliberate["assertions"]["passed"], 90);
    assert_eq!(deliberate["assertions"]["failed"], 0);
    assert_eq!(deliberate["assertions"]["total"], 90);
    assert_eq!(
        deliberate["failureDiagnostics"]["stage"],
        "refreshExactShareActionAuthority"
    );
    assert_eq!(deliberate["failureDiagnostics"]["actionDispatched"], false);
    assert_eq!(
        deliberate["failureDiagnostics"]["systemProbe"]["equality"]["sharedInputSeatActivityObserved"],
        false
    );
    assert_eq!(
        deliberate["appShareHandoff"]["startReceiptAcknowledged"],
        true
    );
    assert_eq!(deliberate["appShareHandoff"]["actionDispatched"], false);
    assert_eq!(
        deliberate["releaseCandidateBinding"]["artifactZipSha256"],
        "8b98da5e53963700f16fb5aeb7e9514dbd2a92723afd3a056b3f064df3f718df"
    );
    let readme = fs::read_to_string(attempt_root.join("README.md")).unwrap();
    assert!(readme.contains("It is not release evidence."));
    assert!(readme.contains("fresh share action authority timed out"));
    assert!(readme.contains("The candidate was not retried."));
}

#[test]
fn withdrawn_v0_12_9_macos_cursor_invariant_attempt_is_byte_exact_and_fail_closed() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.9/computer/attempts/withdrawn-db624da-macos-semantic-hardware-cursor-change",
    );
    let entries = fs::read_dir(attempt_root)
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| {
            let file_type = entry.file_type().unwrap();
            assert!(
                file_type.is_file() && !file_type.is_symlink(),
                "withdrawn v0.12.9 macOS evidence entry must be an ordinary file: {}",
                entry.path().display()
            );
            entry.file_name().to_string_lossy().into_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        entries,
        BTreeSet::from([
            "README.md".to_owned(),
            "computer-01-exact-window-observe.png".to_owned(),
            "helper-results.json".to_owned(),
            "helper-rig.log".to_owned(),
        ])
    );

    for (name, expected_bytes, expected_sha256) in [
        (
            "README.md",
            4_250,
            "a2c8033e8a45e3545dfb5d2a7ed41bbaff304752d3781bf124f67bb18d12bc2b",
        ),
        (
            "computer-01-exact-window-observe.png",
            779_471,
            "550816e6e8a77a3dcfea111e7393c2976b17484609d8e45fe3c170c5883e67ce",
        ),
        (
            "helper-results.json",
            8_164,
            "862184d1d8bce46d64c854a768dadef5efe27747abdb9972c726e5ea6eeb795e",
        ),
        (
            "helper-rig.log",
            4_640,
            "f435b55373a6bfb28a8d002002762b1e174f2af55c541bb1a42f9563ac9160f1",
        ),
    ] {
        let path = attempt_root.join(name);
        assert_eq!(
            fs::metadata(&path).unwrap().len(),
            expected_bytes,
            "withdrawn v0.12.9 macOS evidence size changed: {name}"
        );
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(&path).unwrap())),
            expected_sha256,
            "withdrawn v0.12.9 macOS evidence bytes changed: {name}"
        );
    }

    let results: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("helper-results.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(results["schemaVersion"], 3);
    assert_eq!(results["productVersion"], "0.12.9");
    assert_eq!(results["status"], "failed-release-candidate");
    assert_eq!(
        results["evidenceClass"],
        "release-candidate-negative-result"
    );
    assert_eq!(results["helperSpawnCount"], 1);
    assert_eq!(
        results["fatal"],
        "computer.setValue returned HTTP 504: COMPUTER_OUTCOME_UNKNOWN The command was canceled or failed after computer input dispatch; observe again and do not automatically retry. COMPUTER_BACKGROUND_CONTRACT_VIOLATION: stage=semanticSetValue;failedInvariants=hardwareCursorUnchanged"
    );
    assert_eq!(results["assertions"]["passed"], 40);
    assert_eq!(results["assertions"]["failed"], 0);
    let assertion_details = results["assertions"]["details"].as_array().unwrap();
    assert_eq!(assertion_details.len(), 40);
    assert!(
        assertion_details
            .iter()
            .all(|detail| detail["passed"] == true)
    );

    assert_eq!(results["failureDiagnostics"]["stage"], "notReached");
    assert_eq!(
        results["failureDiagnostics"]["systemProbe"]["baselineCaptured"],
        false
    );
    assert_eq!(
        results["failureDiagnostics"]["systemProbe"]["afterCaptured"],
        false
    );
    assert_eq!(
        results["failureDiagnostics"]["targetSiblingReceiver"]["expectationMet"],
        true
    );
    assert_eq!(
        results["failureDiagnostics"]["targetSiblingReceiver"]["focusedAfter"],
        true
    );
    assert_eq!(
        results["failureDiagnostics"]["targetSiblingReceiver"]["mainAfter"],
        true
    );

    let screenshots = results["screenshots"].as_array().unwrap();
    assert_eq!(screenshots.len(), 1);
    let screenshot = &screenshots[0];
    assert_eq!(screenshot["file"], "computer-01-exact-window-observe.png");
    assert_eq!(
        screenshot["sha256"],
        "550816e6e8a77a3dcfea111e7393c2976b17484609d8e45fe3c170c5883e67ce"
    );
    assert_eq!(screenshot["bytes"], 779_471);
    assert_eq!(screenshot["width"], 1_209);
    assert_eq!(screenshot["height"], 826);
    assert_eq!(screenshot["sourceSequence"], serde_json::Value::Null);
    assert_eq!(screenshot["transportSequence"], serde_json::Value::Null);

    assert_eq!(
        results["packageBinding"]["expectedSha256"],
        "56a87068ac150d322edf923696bd93f4c9ef0f10dd752ef48bfd7ca0e1950500"
    );
    assert_eq!(
        results["packageBinding"]["actualSha256"],
        results["packageBinding"]["expectedSha256"]
    );
    assert_eq!(results["packageBinding"]["expectedSha256Matched"], true);
    assert_eq!(results["packageBinding"]["exactCanonicalAssetSet"], true);
    assert_eq!(results["packageBinding"]["canonicalEntryCount"], 4);
    assert_eq!(results["packageBinding"]["archiveEntryMatched"], true);

    let log = fs::read_to_string(attempt_root.join("helper-rig.log")).unwrap();
    assert_eq!(log.matches(" PASS ").count(), 40);
    assert_eq!(log.matches(" FATAL ").count(), 1);
    assert!(log.contains(
        "COMPUTER_BACKGROUND_CONTRACT_VIOLATION: stage=semanticSetValue;failedInvariants=hardwareCursorUnchanged"
    ));
    for cleanup in [
        "helper stopped",
        "server stopped",
        "fixture stopped",
        "scratch directory removed",
    ] {
        assert!(
            log.contains(cleanup),
            "withdrawn attempt log omits {cleanup}"
        );
    }

    let readme = fs::read_to_string(attempt_root.join("README.md")).unwrap();
    let normalized_readme = readme.split_whitespace().collect::<Vec<_>>().join(" ");
    for truthful_boundary in [
        "Nothing in this directory is evidence for a shipped release.",
        "Windows and stock-Chrome acceptance were not started",
        "zero assertion failures before a fatal post-dispatch refusal",
        "cannot attribute that cursor change to the helper",
        "The candidate was not retried or reused.",
    ] {
        assert!(
            normalized_readme.contains(truthful_boundary),
            "withdrawn attempt README omits truthful boundary: {truthful_boundary}"
        );
    }
}

#[test]
fn latest_macos_candidate_evidence_remains_version_bound_and_reduced() {
    let entries = fs::read_dir("evidence/v0.12.59/computer")
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| {
            let file_type = entry.file_type().unwrap();
            assert!(
                file_type.is_file() && !file_type.is_symlink(),
                "current macOS evidence scaffold entry must be an ordinary file: {}",
                entry.path().display()
            );
            entry.file_name().to_string_lossy().into_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        entries,
        BTreeSet::from([
            "AppShareHandoff.swift".to_owned(),
            "HelperEvidenceFixture.swift".to_owned(),
            "PhysicalPointerHandoff.swift".to_owned(),
            "README.md".to_owned(),
            "SystemProbe.swift".to_owned(),
            "helper-evidence-rig.mjs".to_owned(),
        ])
    );

    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let fixture =
        fs::read_to_string("evidence/v0.12.59/computer/HelperEvidenceFixture.swift").unwrap();
    let readme = fs::read_to_string("evidence/v0.12.59/computer/README.md").unwrap();

    assert!(rig.contains("const EXPECTED_VERSION = \"0.12.59\";"));
    assert!(rig.contains("const EXPECTED_ARCHIVE = `local-browser-bridge-v${EXPECTED_VERSION}-macos-universal.tar.gz`;"));
    assert!(rig.contains("status: \"passed-release-candidate\""));
    assert!(rig.contains("evidenceClass: \"exact-release-candidate-package-live-observation\""));
    assert!(rig.contains("candidateNotice:"));
    assert!(fixture.contains("LBB v0.12.59 Persistent SCStream Evidence"));
    assert!(fixture.contains("var evidenceLane = \"\""));
    assert!(fixture.contains("\"evidence-lane=\\(evidenceLane)\".draw("));
    assert!(fixture.contains("environment[\"LBB_FIXTURE_EVIDENCE_LANE\"]"));
    assert!(fixture.contains("[\"quiet\", \"deliberate-concurrency\"].contains(evidenceLane)"));
    assert!(rig.contains("LBB_FIXTURE_EVIDENCE_LANE: pointerEvidenceLane"));
    assert!(rig.contains("snapshot.evidenceLane === pointerEvidenceLane"));
    assert!(rig.contains("evidenceLane: finalFixtureState.evidenceLane"));
    assert!(rig.contains("acceptanceFinalizerSource"));
    assert!(rig.contains("acceptanceFinalizerSha256: harnessSha256.acceptanceFinalizer"));
    assert!(readme.contains("`evidence-lane=quiet`"));
    assert!(readme.contains("all twelve lane screenshots to have distinct file SHA-256"));
    assert!(readme.contains("distinct canonical decoded-RGBA pixel SHA-256 digests"));
    assert!(readme.contains("macOS v0.12.59 server and helper"));
    assert!(readme.contains("local-browser-bridge-v0.12.59-macos-universal.tar.gz"));
    assert!(!rig.replace("v0.12.59", "").contains("v0.12.1"));
    assert!(!fixture.replace("v0.12.59", "").contains("v0.12.1"));
    assert!(!rig.replace("v0.12.59", "").contains("v0.12.2"));
    assert!(!fixture.replace("v0.12.59", "").contains("v0.12.2"));
    let current_readme = readme
        .split("## Withdrawn v0.12.10 exact-candidate result")
        .next()
        .unwrap();
    assert!(!current_readme.contains("v0.12.1 "));
    assert!(!current_readme.contains("v0.12.1/"));
    assert!(readme.contains("../../v0.12.1/computer/attempts/"));
    assert!(readme.contains(
        "../../v0.12.2/computer/attempts/withdrawn-a52d761-post-cancel-fresh-share-refusal/README.md"
    ));

    let generated = rig
        .split("const GENERATED_OUTPUT_NAMES = [")
        .nth(1)
        .unwrap()
        .split("];\n")
        .next()
        .unwrap();
    let output_names = generated
        .lines()
        .filter_map(|line| line.trim().strip_prefix('"'))
        .filter_map(|line| line.strip_suffix("\","))
        .collect::<BTreeSet<_>>();
    assert_eq!(
        output_names,
        BTreeSet::from([
            "helper-results.json",
            "helper-rig.log",
            "computer-01-exact-window-observe.png",
            "computer-02-semantic-set-value.png",
            "computer-03-semantic-invoke.png",
            "computer-04-persistent-scstream-start.png",
            "computer-05-live-share-pixel-action.png",
            "computer-06-persistent-share-resize.png",
            "operator",
        ])
    );
    assert!(rig.contains("assertNoToken(serialized, \"machine-readable result\")"));
    assert!(rig.contains("assertNoRetainedNativeTextPayload(serialized"));
    assert!(rig.contains("assertNoRetainedPointerRawData(serialized"));
    assert!(rig.contains("every retained screenshot is bound only to the primary exact window"));
    assert!(!rig.contains("computer-07-"));
}

#[test]
fn macos_packaged_evidence_is_bound_to_an_out_of_band_canonical_manifest() {
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let readme = fs::read_to_string("evidence/v0.12.59/computer/README.md")
        .unwrap()
        .replace("\r\n", "\n");
    let binder = fs::read_to_string("scripts/fetch-verify-release-candidate.sh")
        .unwrap()
        .replace("\r\n", "\n");
    let normalized_readme = readme.split_whitespace().collect::<Vec<_>>().join(" ");

    for required in [
        "expectedManifestSha256",
        "<expected-SHA256SUMS-sha256>",
        "CANONICAL_RELEASE_ASSETS",
        "local-browser-bridge-v${EXPECTED_VERSION}-windows-x86_64.exe",
        "local-computer-helper-v${EXPECTED_VERSION}-windows-x86_64.exe",
        "local-browser-bridge-extension-v${EXPECTED_VERSION}.zip",
        "function canonicalChecksumEntries(contents)",
        "contents.includes(\"\\r\")",
        "!contents.endsWith(\"\\n\")",
        "/^([0-9a-f]{64})  ([^\\s/]+)$/",
        "new Set(entries.map((entry) => entry.file)).size !== entries.length",
        "manifestBinding.expectedSha256Matched = manifestSha256 === expectedSha256",
        "checksum manifest has the exact canonical four-entry set",
        "archive checksum is bound by the canonical manifest",
        "checksumManifest: manifestBinding",
        "packageBinding: manifestBinding",
        "Candidate parsing and the first invocation of either supplied executable",
        "async function preparePackage()",
        "status: \"prepared-package-without-candidate-execution\"",
        "candidateBytesExecuted: false",
    ] {
        assert!(
            rig.contains(required),
            "macOS evidence manifest binding is missing: {required}"
        );
    }
    for required in [
        "checked-in candidate binder validates",
        "raw artifact ZIP size and SHA-256",
        "exact five-file inventory",
        "canonical LF checksum manifest",
        "all five GitHub attestations",
        "both exact-attempt attestation URI fields",
        "Before invoking either supplied candidate executable—even with `--version`",
        "No supplied server or helper code executes before",
        "Extraction and candidate execution are permitted only below this line",
        "scripts/fetch-verify-release-candidate.sh",
        "candidate-binding.json",
    ] {
        assert!(
            normalized_readme.contains(required),
            "macOS evidence manifest handoff is undocumented: {required}"
        );
    }
    for required in [
        "RUN_ATTEMPT",
        "ARTIFACT_ID",
        "SOURCE_SHA",
        "WORKFLOW_REF=\"refs/heads/main\"",
        "release-candidate",
        "raw artifact ZIP size mismatch",
        "outer artifact ZIP inventory changed before bounded extraction",
        "checksum manifest line count mismatch",
        "candidate payload checksum mismatch",
        "gh attestation verify",
        "--source-ref \"$WORKFLOW_REF\"",
        "--source-digest \"$SOURCE_SHA\"",
        "--signer-workflow \"$REPOSITORY/.github/workflows/deploy.yml\"",
        "--deny-self-hosted-runners",
        ".verificationResult.statement.predicate.runDetails.metadata.invocationId",
        ".verificationResult.signature.certificate.runInvocationURI",
        ".verificationResult.signature.certificate.runnerEnvironment == \"github-hosted\"",
        "candidate-binding.json",
    ] {
        assert!(
            binder.contains(required),
            "checked-in candidate binder is missing: {required}"
        );
    }

    let main = rig.split("async function main() {").nth(1).unwrap();
    let binding_complete = main
        .find("requireCheck(\"supplied helper is archive-exact\"")
        .unwrap();
    let first_candidate_inspection = main
        .find("const serverArchitectures = architectures(serverPath)")
        .unwrap();
    let first_target_execution = [
        "exactVersion(serverPath, \"local-browser-bridge\")",
        "exactVersion(helperPath, \"local-computer-helper\")",
        "spawn(serverPath, [\"--no-update-check\"]",
        "startHelperOnce(helperEnvironment)",
    ]
    .into_iter()
    .map(|needle| main.find(needle).unwrap())
    .min()
    .unwrap();
    assert!(
        binding_complete < first_candidate_inspection
            && first_candidate_inspection < first_target_execution,
        "candidate package was inspected or executed before exact archive binding"
    );
    let raw_artifact_digest = binder.find("ARTIFACT_ZIP_SHA256=$(sha256_file").unwrap();
    let exact_inventory = binder
        .find("outer artifact ZIP inventory changed before bounded extraction")
        .unwrap();
    let canonical_manifest = binder
        .find("checksum manifest line count mismatch")
        .unwrap();
    let all_checksums = binder.find("shasum -a 256 -c SHA256SUMS.txt").unwrap();
    let package_verifier = binder.find("scripts/verify-release-assets.sh").unwrap();
    let all_attestations = binder.find("gh attestation verify").unwrap();
    let create_binding = binder
        .find("BINDING=\"$DESTINATION/candidate-binding.json\"")
        .unwrap();
    assert!(
        raw_artifact_digest < exact_inventory
            && exact_inventory < canonical_manifest
            && canonical_manifest < all_checksums
            && all_checksums < package_verifier
            && package_verifier < all_attestations
            && all_attestations < create_binding,
        "candidate binder must finish artifact, payload, policy, and exact-attempt provenance checks before publishing its binding"
    );

    let binder_invocation = readme
        .find("bash scripts/fetch-verify-release-candidate.sh")
        .unwrap();
    let first_documented_extraction = readme.find("\n  --prepare-package \\\n").unwrap();
    let first_documented_execution = readme
        .find("\n  \"$SERVER\" \"$HELPER\" \"$QUIET_DIR\"")
        .unwrap();
    assert!(
        binder_invocation < first_documented_extraction
            && first_documented_extraction < first_documented_execution,
        "README must run the checked-in candidate binder before extraction or execution"
    );
    assert!(
        !readme.contains("tar -xzf") && !readme.contains("tar --extract"),
        "README must not bypass the bounded package preparer with raw tar extraction"
    );
}

#[test]
fn macos_packaged_evidence_streams_one_exact_bounded_pax_free_archive() {
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "const MAX_COMPRESSED_MACOS_ARCHIVE_BYTES = 256 * 1024 * 1024;",
        "const MAX_UNCOMPRESSED_MACOS_ARCHIVE_BYTES = 512 * 1024 * 1024;",
        "const MAX_CHECKSUM_MANIFEST_BYTES = 16 * 1024;",
        "const EXPECTED_MACOS_ARCHIVE_ENTRIES = [",
        "function extractCandidateArchiveBounded(path, destination, expectedArchiveSha256)",
        "with gzip.GzipFile(fileobj=archive_stream, mode=\"rb\") as tar_stream:",
        "os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, \"O_NOFOLLOW\", 0)",
        "macOS tar is not the expected PAX-free ustar format",
        "macOS tar contains a link, PAX record, or unsupported member type",
        "macOS tar contains a duplicate member",
        "macOS tar exceeds the total uncompressed payload bound",
        "macOS tar member mode is not exact",
        "macOS archive changed during bounded extraction",
        "summary.entryCount === EXPECTED_MACOS_ARCHIVE_ENTRIES.length",
        "seventeen PAX-free regular-file/directory entries passed bounded streaming extraction",
        "extractCandidateArchiveBounded(\n    archivePath,\n    archiveExtractRoot,\n    manifest.archiveSha256,",
        "extractCandidateArchiveBounded(\n      archivePath,\n      outputDir,\n      manifest.archiveSha256,",
        "os.fchmod(output_descriptor, entry[\"mode\"])",
        "os.fchmod(directory_descriptor, entry[\"mode\"])",
    ] {
        assert!(
            rig.contains(required),
            "bounded macOS candidate archive gate is missing `{required}`"
        );
    }

    let expected_inventory = rig
        .split("const EXPECTED_MACOS_ARCHIVE_ENTRIES = [")
        .nth(1)
        .unwrap()
        .split("];\n")
        .next()
        .unwrap();
    assert_eq!(
        expected_inventory.matches("{ name:").count(),
        17,
        "macOS package inventory must contain exactly seventeen entries"
    );
    for required_path in [
        "local-browser-bridge",
        "Local Browser Bridge.app/Contents/Info.plist",
        "Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop",
        "Local Browser Bridge.app/Contents/_CodeSignature/CodeResources",
        "Local Computer Helper.app/Contents/Info.plist",
        "Local Computer Helper.app/Contents/MacOS/local-computer-helper",
        "Local Computer Helper.app/Contents/_CodeSignature/CodeResources",
        "LICENSE",
        "THIRD_PARTY_LICENSES.txt",
    ] {
        assert!(expected_inventory.contains(required_path));
    }
    for forbidden in [
        "run(\"tar\"",
        "[\"-tzf\", archivePath]",
        "[\"-tvzf\", archivePath]",
        "[\"-xzf\", archivePath",
    ] {
        assert!(
            !rig.contains(forbidden),
            "macOS evidence rig retained unbounded tar handling: {forbidden}"
        );
    }

    let main = rig.split("async function main() {").nth(1).unwrap();
    let canonical_manifest = main.find("bindCanonicalChecksumManifest(").unwrap();
    let bounded_extraction = main.find("extractCandidateArchiveBounded(").unwrap();
    let first_candidate_execution = main.find("exactVersion(serverPath").unwrap();
    assert!(
        canonical_manifest < bounded_extraction && bounded_extraction < first_candidate_execution
    );
}

#[cfg(unix)]
#[test]
fn macos_package_preparer_accepts_only_the_canonical_bounded_ustar_package() {
    use std::os::unix::fs::PermissionsExt;

    fn set_mode(path: &std::path::Path, mode: u32) {
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    fn collect_inventory(
        root: &std::path::Path,
        directory: &std::path::Path,
        inventory: &mut BTreeSet<String>,
    ) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            inventory.insert(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
            if entry.file_type().unwrap().is_dir() {
                collect_inventory(root, &path, inventory);
            }
        }
    }

    let repository = std::env::current_dir().unwrap();
    let rig = repository.join("evidence/v0.12.59/computer/helper-evidence-rig.mjs");
    let temporary = tempfile::tempdir().unwrap();
    set_mode(temporary.path(), 0o700);
    let generator = temporary.path().join("make-package.py");
    fs::write(
        &generator,
        r#"import io
import sys
import tarfile

scenario, archive_path = sys.argv[1:]
server = b'#!/bin/sh\nprintf executed > "$(dirname "$0")/EXECUTED"\nexit 91\n'
desktop = b'#!/bin/sh\nprintf executed > "$(dirname "$0")/DESKTOP_EXECUTED"\nexit 91\n'
helper = b'#!/bin/sh\nprintf executed > "$(dirname "$0")/HELPER_EXECUTED"\nexit 91\n'
entries = [
    ["local-browser-bridge", "file", 0o755, server],
    ["Local Browser Bridge.app", "directory", 0o755, b""],
    ["Local Browser Bridge.app/Contents", "directory", 0o755, b""],
    ["Local Browser Bridge.app/Contents/Info.plist", "file", 0o644, b"plist"],
    ["Local Browser Bridge.app/Contents/MacOS", "directory", 0o755, b""],
    ["Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop", "file", 0o755, desktop],
    ["Local Browser Bridge.app/Contents/_CodeSignature", "directory", 0o755, b""],
    ["Local Browser Bridge.app/Contents/_CodeSignature/CodeResources", "file", 0o644, b"signature"],
    ["Local Computer Helper.app", "directory", 0o755, b""],
    ["Local Computer Helper.app/Contents", "directory", 0o755, b""],
    ["Local Computer Helper.app/Contents/Info.plist", "file", 0o644, b"plist"],
    ["Local Computer Helper.app/Contents/MacOS", "directory", 0o755, b""],
    ["Local Computer Helper.app/Contents/MacOS/local-computer-helper", "file", 0o755, helper],
    ["Local Computer Helper.app/Contents/_CodeSignature", "directory", 0o755, b""],
    ["Local Computer Helper.app/Contents/_CodeSignature/CodeResources", "file", 0o644, b"signature"],
    ["LICENSE", "file", 0o644, b"license"],
    ["THIRD_PARTY_LICENSES.txt", "file", 0o644, b"third-party"],
]
if scenario == "duplicate":
    entries.append(entries[0].copy())
if scenario == "traversal":
    entries[8][0] = "../escaped"
if scenario == "mode":
    entries[0][2] = 0o644
if scenario == "oversize":
    entries[3][3] = b"x" * (1024 * 1024 + 1)

archive_format = tarfile.PAX_FORMAT if scenario == "pax" else tarfile.USTAR_FORMAT
with tarfile.open(archive_path, "w:gz", format=archive_format) as archive:
    for index, (name, kind, mode, payload) in enumerate(entries):
        member = tarfile.TarInfo(name)
        member.mode = mode
        member.uid = 0
        member.gid = 0
        member.uname = ""
        member.gname = ""
        if kind == "directory":
            member.type = tarfile.DIRTYPE
            member.size = 0
            archive.addfile(member)
        else:
            member.type = tarfile.REGTYPE
            member.size = len(payload)
            if scenario == "pax" and index == 0:
                member.pax_headers = {"comment": "forbidden-pax-record"}
            archive.addfile(member, io.BytesIO(payload))
"#,
    )
    .unwrap();
    set_mode(&generator, 0o600);

    let mut valid_output = None;
    for (scenario, expected_failure) in [
        ("valid", None),
        ("pax", Some("link, PAX record, or unsupported member type")),
        ("duplicate", Some("contains a duplicate member")),
        ("traversal", Some("noncanonical or traversal-capable path")),
        ("mode", Some("member mode is not exact")),
        ("oversize", Some("member exceeds its byte bound")),
    ] {
        let case_root = temporary.path().join(scenario);
        fs::create_dir(&case_root).unwrap();
        set_mode(&case_root, 0o700);
        let archive = case_root.join("local-browser-bridge-v0.12.59-macos-universal.tar.gz");
        let generated = Command::new("python3")
            .arg(&generator)
            .arg(scenario)
            .arg(&archive)
            .output()
            .unwrap();
        assert!(
            generated.status.success(),
            "archive generator failed for {scenario}: {}",
            String::from_utf8_lossy(&generated.stderr)
        );
        set_mode(&archive, 0o600);
        let archive_sha256 = file_sha256(archive.to_str().unwrap());
        let zero_hash = "0".repeat(64);
        let manifest_text = format!(
            "{zero_hash}  local-browser-bridge-v0.12.59-windows-x86_64.exe\n\
             {zero_hash}  local-computer-helper-v0.12.59-windows-x86_64.exe\n\
             {archive_sha256}  local-browser-bridge-v0.12.59-macos-universal.tar.gz\n\
             {zero_hash}  local-browser-bridge-extension-v0.12.59.zip\n"
        );
        let manifest = case_root.join("SHA256SUMS.txt");
        fs::write(&manifest, &manifest_text).unwrap();
        set_mode(&manifest, 0o600);
        let manifest_sha256 = format!("{:x}", Sha256::digest(manifest_text.as_bytes()));
        let output_dir = case_root.join("prepared");
        let prepared = Command::new("node")
            .arg(&rig)
            .arg("--prepare-package")
            .arg(&archive)
            .arg(&manifest)
            .arg(&manifest_sha256)
            .arg(&output_dir)
            .output()
            .unwrap();

        if let Some(expected_failure) = expected_failure {
            assert!(
                !prepared.status.success(),
                "unsafe {scenario} archive unexpectedly passed package preparation"
            );
            assert!(
                String::from_utf8_lossy(&prepared.stderr).contains(expected_failure),
                "unsafe {scenario} archive failed for the wrong reason: {}",
                String::from_utf8_lossy(&prepared.stderr)
            );
            assert!(
                !output_dir.exists(),
                "failed {scenario} preparation retained a partial output"
            );
            assert!(!case_root.join("escaped").exists());
            continue;
        }

        assert!(
            prepared.status.success(),
            "canonical archive preparation failed: {}",
            String::from_utf8_lossy(&prepared.stderr)
        );
        let stdout = String::from_utf8_lossy(&prepared.stdout);
        assert!(stdout.contains("prepared-package-without-candidate-execution"));
        assert!(stdout.contains("\"candidateBytesExecuted\":false"));
        assert_eq!(
            fs::metadata(&output_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(output_dir.join("local-browser-bridge"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert_eq!(
            fs::metadata(output_dir.join("Local Browser Bridge.app/Contents/Info.plist"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
        assert_eq!(
            fs::metadata(output_dir.join("Local Computer Helper.app/Contents/Info.plist"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o644
        );
        assert!(!output_dir.join("EXECUTED").exists());
        assert!(
            !output_dir
                .join("Local Browser Bridge.app/Contents/MacOS/DESKTOP_EXECUTED")
                .exists()
        );
        assert!(
            !output_dir
                .join("Local Computer Helper.app/Contents/MacOS/HELPER_EXECUTED")
                .exists()
        );
        let mut inventory = BTreeSet::new();
        collect_inventory(&output_dir, &output_dir, &mut inventory);
        assert_eq!(
            inventory,
            BTreeSet::from([
                "LICENSE".to_string(),
                "Local Browser Bridge.app".to_string(),
                "Local Browser Bridge.app/Contents".to_string(),
                "Local Browser Bridge.app/Contents/Info.plist".to_string(),
                "Local Browser Bridge.app/Contents/MacOS".to_string(),
                "Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop".to_string(),
                "Local Browser Bridge.app/Contents/_CodeSignature".to_string(),
                "Local Browser Bridge.app/Contents/_CodeSignature/CodeResources".to_string(),
                "Local Computer Helper.app".to_string(),
                "Local Computer Helper.app/Contents".to_string(),
                "Local Computer Helper.app/Contents/Info.plist".to_string(),
                "Local Computer Helper.app/Contents/MacOS".to_string(),
                "Local Computer Helper.app/Contents/MacOS/local-computer-helper".to_string(),
                "Local Computer Helper.app/Contents/_CodeSignature".to_string(),
                "Local Computer Helper.app/Contents/_CodeSignature/CodeResources".to_string(),
                "THIRD_PARTY_LICENSES.txt".to_string(),
                "local-browser-bridge".to_string(),
            ])
        );
        valid_output = Some((archive, manifest, manifest_sha256, output_dir));
    }

    let (archive, manifest, manifest_sha256, output_dir) = valid_output.unwrap();
    let reused = Command::new("node")
        .arg(&rig)
        .arg("--prepare-package")
        .arg(&archive)
        .arg(&manifest)
        .arg(&manifest_sha256)
        .arg(&output_dir)
        .output()
        .unwrap();
    assert!(
        !reused.status.success(),
        "an existing package output was accepted"
    );
    assert!(String::from_utf8_lossy(&reused.stderr).contains("already exists"));

    let mismatch_output = temporary.path().join("manifest-mismatch-output");
    let mismatch = Command::new("node")
        .arg(&rig)
        .arg("--prepare-package")
        .arg(&archive)
        .arg(&manifest)
        .arg("f".repeat(64))
        .arg(&mismatch_output)
        .output()
        .unwrap();
    assert!(
        !mismatch.status.success(),
        "an unbound manifest was accepted"
    );
    assert!(
        String::from_utf8_lossy(&mismatch.stderr)
            .contains("out-of-band checksum-manifest hash matches")
    );
    assert!(!mismatch_output.exists());
}

#[test]
fn macos_packaged_evidence_uses_a_clean_source_harness_and_fresh_lane_outputs() {
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "expectedSourceSha",
        "expectedWorkflowRunId",
        "expectedWorkflowRunAttempt",
        "expectedArtifactId",
        "expectedArtifactZipSha256",
        "fileURLToPath(import.meta.url)",
        "const rigSourceDirectory = dirname(rigSourcePath)",
        "resolve(rigSourceDirectory, \"HelperEvidenceFixture.swift\")",
        "output parent is an owner-private ordinary directory",
        "(outputParentState.mode & 0o077) === 0",
        "await mkdir(outputDir, { recursive: false, mode: 0o700 })",
        "output directory is newly created and owner-private",
        "flag: \"wx\", mode: 0o600",
        "verifyHarnessSourceBinding(\"pre-run\")",
        "verifyHarnessSourceBinding(\"post-run\")",
        "status\", \"--porcelain=v2\", \"--untracked-files=all",
        "diff\", \"--quiet\", \"HEAD\", \"--",
        "diff\", \"--cached\", \"--quiet",
        "ls-files\", \"--deleted",
        "ls-files\", \"--others\", \"--exclude-standard",
        "fsck\", \"--full",
        "rev-parse\", `HEAD:${trackedPath}`",
        "hash-object\", \"--\", sourcePath",
        "releaseCandidateBinding: { ...releaseCandidateBinding }",
        "harnessSourceBinding: { ...harnessSourceBinding }",
    ] {
        assert!(
            rig.contains(required),
            "clean harness binding is missing {required}"
        );
    }
    assert!(!rig.contains("resolve(outputDir, \"HelperEvidenceFixture.swift\")"));
    assert!(!rig.contains("resolve(outputDir, \"helper-evidence-rig.mjs\")"));

    let main = rig.split("async function main() {").nth(1).unwrap();
    let reserve = main
        .find("await mkdir(outputDir, { recursive: false, mode: 0o700 })")
        .unwrap();
    let source_gate = main
        .find("verifyHarnessSourceBinding(\"pre-run\")")
        .unwrap();
    let scratch = main.find("scratchDir = await mkdtemp").unwrap();
    let first_execution = main.find("exactVersion(serverPath").unwrap();
    let final_source_gate = main
        .find("verifyHarnessSourceBinding(\"post-run\")")
        .unwrap();
    assert!(reserve < source_gate && source_gate < scratch && scratch < first_execution);
    assert!(first_execution < final_source_gate);
}

#[test]
fn macos_resize_evidence_requires_a_settled_geometry_bound_frame() {
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs").unwrap();
    assert!(rig.contains("capturedFrameMatchesWindowGeometry"));
    assert!(rig.contains("share-resize-settled"));
    assert!(rig.contains("sample.sourceSequence > resizeTransition.sample.sourceSequence"));
    assert!(rig.contains("captured PNG dimensions do not match the bound observation"));
    assert!(rig.contains("resize screenshot dimensions match settled observation"));
    assert!(rig.contains("resize screenshot geometry changed"));
    assert!(!rig.contains("resize screenshot changed"));
}

#[test]
fn macos_packaged_evidence_acts_types_and_explicitly_cancels_fail_closed() {
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    assert!(rig.contains("function childEnvironment(overrides = {})"));
    assert!(!rig.contains("...process.env"));
    let fixture = fs::read_to_string("evidence/v0.12.59/computer/HelperEvidenceFixture.swift")
        .unwrap()
        .replace("\r\n", "\n");

    let generated_outputs = rig
        .split("const GENERATED_OUTPUT_NAMES = [")
        .nth(1)
        .unwrap()
        .split("];\n")
        .next()
        .unwrap();
    assert_eq!(generated_outputs.matches("computer-0").count(), 6);
    let resize_screenshot = rig.find("computer-06-persistent-share-resize.png").unwrap();
    let post_resize_action = rig
        .find("const beforePostResizeAction = current.sample")
        .unwrap();
    assert!(resize_screenshot < post_resize_action);

    let post_resize = rig
        .split("const beforePostResizeAction = current.sample")
        .nth(1)
        .unwrap()
        .split("const nativeTextSetupSystemBefore")
        .next()
        .unwrap();
    for required in [
        "commandResponse(\n    \"computer.click\"",
        "post-resize pixel click target-side proof",
        "postResizeFixtureAfter.clicks === 2",
        "postResizeFixtureAfter.semanticPresses === postResizeFixtureBefore.semanticPresses",
        "postResizeFixtureAfter.resizeCount === postResizeFixtureBefore.resizeCount",
        "postResizeClick.frameId === observation.frameId",
        "postResizeClick.shareId === firstShareId",
        "postResizeClick.sourceSequence === beforePostResizeAction.sourceSequence",
        "requireIndependentInvariants(\"post-resize pixel action\", postResizeActionInvariants)",
        "share-after-post-resize-action",
        "capturedFrameMatchesWindowGeometry(sample)",
    ] {
        assert!(
            post_resize.contains(required),
            "missing post-resize proof: {required}"
        );
    }

    for required in [
        "var focusCount = 0",
        "func focusSemanticField(controlSequence: Int, siblingView: SiblingView) -> Bool",
        "window.makeFirstResponder(semanticField)",
        "siblingView.prepareAsFocusedSibling()",
        "editor.setSelectedRange",
        "control.action == \"focus-semantic-field\"",
        "var moveEvents = 0",
        "private final class FixtureWindow: NSWindow",
        "override func sendEvent(_ event: NSEvent)",
        "event.type == .mouseMoved, event.windowNumber == windowNumber",
        "recordExactMouseMoved?()",
        "func recordExactWindowMoveDelivery()",
        "window.recordExactMouseMoved = { [weak view] in",
        "state.moveEvents < 1_000_000",
        "window.acceptsMouseMovedEvents = true",
    ] {
        assert!(
            fixture.contains(required),
            "missing fixture focus proof: {required}"
        );
    }
    assert!(!fixture.contains("NSTrackingArea"));
    let native_text = rig
        .split("const nativeTextSetupSystemBefore")
        .nth(1)
        .unwrap()
        .split("validateShareSamples(shareSamples);")
        .next()
        .unwrap();
    for required in [
        "action: \"focus-semantic-field\"",
        "background fixture field prepared without mutation",
        "command(\"computer.typeText\"",
        "native typeText exact fixture read-back",
        "nativeText.effect === \"Unverifiable\"",
        "nativeText.characters === NATIVE_TEXT_SUFFIX.length",
        "nativeText.utf16CodeUnits === NATIVE_TEXT_SUFFIX.length",
        "requireIndependentInvariants(\"native typeText\", nativeTextInvariants)",
        "share-after-native-text-delivery",
        "candidateObservation.shareId === firstShareId",
        "candidateObservation.sourceSequence === sample.sourceSequence",
        "candidateObservation.elements.some",
        "native text restore frame is bound to the persistent share authority",
        "restoreObservation.shareId === firstShareId",
        "restoreObservation.sourceSequence === current.sample.sourceSequence",
        "command(\"computer.setValue\"",
        "native text fixture value restored",
        "native text restore exact fixture state",
        "share-after-native-text-restore",
    ] {
        assert!(
            native_text.contains(required),
            "missing native text proof: {required}"
        );
    }
    let native_text_action = native_text.find("command(\"computer.typeText\"").unwrap();
    let restore_share_frame = native_text
        .find("share-after-native-text-delivery")
        .unwrap();
    let restore_action = native_text.find("command(\"computer.setValue\"").unwrap();
    assert!(native_text_action < restore_share_frame && restore_share_frame < restore_action);
    assert!(!native_text.contains("restoreObserved"));
    assert!(!native_text.contains("freshObserve(targetWindow.id)"));

    assert!(rig.contains("cancellation starts from a current resized share frame"));
    let pre_cancellation = rig
        .split("const shareStatusBody = await command(\"computer.share.status\");")
        .nth(1)
        .unwrap()
        .split("const cancellationStartedAt = Date.now();")
        .next()
        .unwrap();
    for required in [
        "const preCancellationShareSample = current.sample",
        "const cancellationFixtureBefore = await fixtureState(fixtureStatePath)",
        "const cancellationSystemBefore = processProbe(systemProbeBinary)",
        "fresh cancellation action frame",
        "sample.sequence > preCancellationShareSample.sequence",
        "sample.sourceSequence > preCancellationShareSample.sourceSequence",
        "candidateObservation.sourceSequence === sample.sourceSequence",
        "capturedFrameMatchesWindowGeometry(sample)",
        "const canceledFrameId = observation.frameId",
    ] {
        assert!(
            pre_cancellation.contains(required),
            "missing fresh cancellation authority proof: {required}"
        );
    }
    assert!(
        pre_cancellation
            .find("const cancellationSystemBefore")
            .unwrap()
            < pre_cancellation
                .find("fresh cancellation action frame")
                .unwrap()
    );
    assert!(
        pre_cancellation
            .find("fresh cancellation action frame")
            .unwrap()
            < pre_cancellation.find("const canceledFrameId").unwrap()
    );
    let cancellation = rig
        .split("const cancellationStartedAt = Date.now();")
        .nth(1)
        .unwrap()
        .split("const targetCloseShareStartBody")
        .next()
        .unwrap();
    for required in [
        "commandResponse(\n    \"computer.move\"",
        "CALL_IN_PROGRESS",
        "fixture target-routed cancellation move dispatch",
        "snapshot.moveEvents > cancellationFixtureBefore.moveEvents",
        "CANCELLATION_DISPATCH_PROOF_TIMEOUT_MS",
        "Promise.race([",
        "originalCanceledRequest.then((response) =>",
        "computer.move completed before target delivery",
        "cancellation waits for target-routed native move delivery",
        "cancelCommandResponse(cancellationCallId)",
        "cancelAccepted.status === 202",
        "COMMAND_OUTCOME_UNKNOWN",
        "outcome_unknown",
        "recoveryHint === \"reobserve\"",
        "CALL_NOT_IN_PROGRESS",
        "replayedCanceled.body.replayed === true",
        "JSON.stringify(replayedWithoutMarker) === JSON.stringify(canceledOriginal.body)",
        "CALL_ID_REUSED",
        "snapshot.computer?.sessionId === hello.sessionId",
        "computerObservation === null",
        "NO_COMPUTER_SCREENSHOT",
        "command(\"computer.share.stop\")",
        "explicitStop.reason === \"not-active\"",
        "revoked authority gates pre-recovery mutations before helper relay",
        "NO_COMPUTER_FRAME",
        "gated old-frame action cannot recreate a computer surface",
        "freshObserve(targetWindow.id)",
        "explicit one-shot observation recovers exact-session authority",
        "recoveryObservation.frameId !== canceledFrameId",
        "COMPUTER_STALE_FRAME",
        "pre-cancellation frame stays stale after explicit recovery",
        "rejected stale action preserves the recovered exact frame",
        "canceled move, gated refusal, recovery, and stale refusal caused no functional fixture mutation",
        "requireIndependentInvariants(\"cancellation/stop\", cancellationInvariants)",
    ] {
        assert!(
            cancellation.contains(required),
            "missing explicit cancellation proof: {required}"
        );
    }
    for forbidden in [
        "CANCELLATION_TEXT_CHARACTERS",
        "PRODUCTION_COMMAND_TIMEOUT_FLOOR_MS",
        "CANCEL_AFTER_START_MS",
        "await delay(CANCEL_AFTER_START_MS)",
        "LBB_COMPUTER_TEST",
        "mock cancellation",
        "semanticValue: finalFixtureState.semanticValue",
        "payloadPersisted",
    ] {
        assert!(
            !rig.contains(forbidden),
            "packaged evidence uses an impossible, synthetic, or unsafe shortcut: {forbidden}"
        );
    }
    assert!(rig.contains("schemaVersion: 9"));
    assert!(
        rig.contains("const NATIVE_TEXT_SUFFIX = `-native-${randomBytes(6).toString(\"hex\")}`;")
    );
    assert!(rig.contains("contentRetainedInEvidenceOutputs: false"));
    assert!(rig.contains("temporaryScratchFixtureStateUsed: true"));
    assert!(rig.contains("assertNoRetainedNativeTextPayload(serialized"));
    assert!(rig.contains("assertNoRetainedNativeTextPayload(persistedLog"));
    assert!(rig.contains("text.split(NATIVE_TEXT_SUFFIX).join(\"[NATIVE_TEXT_PAYLOAD]\")"));
    assert!(rig.contains("nativeTextPayloadMayBeVisible = true"));
}

#[test]
fn macos_packaged_evidence_closes_exact_target_under_a_live_share_fail_closed() {
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let readme = fs::read_to_string("evidence/v0.12.59/computer/README.md")
        .unwrap()
        .replace("\r\n", "\n");
    let normalized_readme = readme.split_whitespace().collect::<Vec<_>>().join(" ");

    let cancellation_start = rig.find("const cancellationStartedAt").unwrap();
    let recovery_observe = rig.find("const recoveryObserve").unwrap();
    let recovered_state = rig.find("const recoveredState").unwrap();
    let share_start = rig.find("const targetCloseShareStartBody").unwrap();
    let share_start_observation = rig.find("const targetCloseShareStartObservation").unwrap();
    let active_frame = rig.find("const targetCloseActive").unwrap();
    let target_termination = rig.find("const fixtureTargetClosure").unwrap();
    let terminal_state = rig.find("const targetClosedState").unwrap();
    let stale_refusal = rig.find("const targetClosedStaleAction").unwrap();
    let closed_observe = rig.find("const targetClosedObserve").unwrap();
    let target_absent = rig.find("const targetClosedStatusBody").unwrap();
    let helper_teardown = rig.find("const helperTeardown").unwrap();
    assert!(
        cancellation_start < recovery_observe
            && recovery_observe < recovered_state
            && recovered_state < share_start
            && share_start < share_start_observation
            && share_start_observation < active_frame
            && active_frame < target_termination
            && target_termination < terminal_state
            && terminal_state < stale_refusal
            && stale_refusal < closed_observe
            && closed_observe < target_absent
            && target_absent < helper_teardown,
        "exact-target close evidence must follow the live share boundary and precede generic teardown"
    );

    let recovery_to_close = &rig[recovery_observe..target_termination];
    for required in [
        "stage: \"freshShareAfterCancellationRecovery\"",
        "fresh share start retires the recovered one-shot frame under exact new authority",
        "targetCloseShareStartState.computer?.sessionId === hello.sessionId",
        "targetCloseShareStartState.computer?.share?.id === targetCloseShare.id",
        "targetCloseShareStartObservation?.frameId !== recoveryObservation.frameId",
        "targetCloseShareStartObservation?.shareId == null",
        "targetCloseShareStartObservation?.sourceSequence == null",
        "targetCloseShareStartObservation?.share?.id === targetCloseShare.id",
        "targetCloseObservation.frameId !== recoveryObservation.frameId",
        "targetCloseObservation.frameId !== targetCloseShareStartObservation.frameId",
        "targetCloseObservation.sourceSequence > 0",
    ] {
        assert!(
            recovery_to_close.contains(required),
            "fresh-share recovery regression is missing: {required}"
        );
    }

    let pre_close = &rig[active_frame..target_termination];
    assert!(pre_close.contains("targetCloseObservation.share?.active === true"));
    assert!(pre_close.contains("targetCloseObservation.share?.id === targetCloseShare.id"));
    assert!(pre_close.contains("fixtureProcess?.exitCode === null"));
    assert!(
        !pre_close.contains("delay("),
        "target termination must be causally gated by a live exact-share frame, not a fixed sleep"
    );

    let target_close = &rig[share_start..helper_teardown];
    for required in [
        "targetCloseShare.id !== firstShareId",
        "targetCloseShare.windowId === targetWindow.id",
        "targetCloseShare.pid === fixtureReady.pid",
        "sample?.shareId === targetCloseShare.id",
        "terminate(fixtureProcess, \"exact fixture target\")",
        "fixtureTargetClosure.requested === true",
        "fixtureTargetClosure.alreadyExited === false",
        "!targetClosedStatus.windows.some",
        "window.id === targetWindow.id && window.pid === fixtureReady.pid",
        "share?.reason === \"capture-error\"",
        "share?.id === targetCloseShare.id",
        "TARGET_CLOSE_CAPTURE_CODES.has(share?.code)",
        "helperProcess?.exitCode === null",
        "helperSpawnCount === 1",
        "NO_COMPUTER_SCREENSHOT",
        "TARGET_CLOSE_SETTLE_FRAME_PERIODS * 1_000",
        "closed target cannot republish a queued native frame",
        "targetClosedStaleAction.body.error?.code === \"NO_COMPUTER_FRAME\"",
        "closed-target frame is refused before helper action relay",
        "targetClosedAfterStaleAction.computerObservation === null",
        "targetClosedObserve.body.error?.code === \"COMPUTER_NO_WINDOW\"",
        "targetClosedObserve.body.taxonomy?.code === \"target_changed\"",
        "closed-target observe refusal preserves terminal teardown",
        "targetCloseStop.reason === \"not-active\"",
        "target-close cleanup leaves no share or frame authority",
        "requireIndependentInvariants(\"target-close\", targetCloseInvariants)",
        "fixtureProcess = null",
    ] {
        assert!(
            target_close.contains(required),
            "missing exact-target close proof: {required}"
        );
    }
    assert!(rig.contains("\"COMPUTER_NO_WINDOW\", \"COMPUTER_CAPTURE_FAILED\""));
    assert!(rig.contains("exactTargetClose: {"));
    assert!(rig.contains("freshPersistentShare: {"));
    assert!(rig.contains("recoveredOneShotFrameRetired:"));
    assert!(rig.contains("firstStreamFrameRetiredOneShotAuthority:"));
    assert!(rig.contains("helperActionRelayed: false"));
    assert!(
        rig.contains(
            "queuedFrameRepublished: targetClosedSettledState.computerObservation !== null"
        )
    );
    assert!(
        !target_close.contains("saveCurrentScreenshot("),
        "target-close evidence must not retain a target image after native-text delivery"
    );

    for required in [
        "replace the recovered one-shot frame with a distinct exact-window one-shot",
        "frame ID differs from both one-shot frames",
        "live regression for the v0.12.2 post-cancellation fresh-share",
        "While that share is active, the runner sends `SIGTERM` only to its spawned",
        "The server must mark the share stopped with",
        "`NO_COMPUTER_FRAME`",
        "`COMPUTER_NO_WINDOW`",
        "unrelated application or window",
        "only after it has requested and observed exit of its exact spawned fixture",
    ] {
        assert!(
            normalized_readme.contains(required),
            "evidence README is missing target-close contract: {required}"
        );
    }
}

#[test]
fn macos_system_probe_anchors_user_focus_to_the_ax_focused_onscreen_window() {
    let probe = fs::read_to_string("evidence/v0.12.59/computer/SystemProbe.swift")
        .unwrap()
        .replace("\r\n", "\n");
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "private func selectedForegroundWindowIdentifier(",
        "$0.ownerPID == pid && $0.layer == 0 && $0.alpha > 0 && $0.identifier > 0",
        "$0.identifier == preferredFocusedWindowID",
        "return visibleApplicationWindows.first?.identifier ?? 0",
        "a same-process app-share auxiliary window must not replace the AX-focused window",
        "an off-screen or foreign preferred identity must not be trusted",
        "macOS system probe foreground-window self-test passed",
        "preferredFocusedWindowID: foregroundAXFocusedWindowID == foregroundAXMainWindowID",
    ] {
        assert!(
            probe.contains(required),
            "SystemProbe focused-window selection omits {required}"
        );
    }
    assert!(rig.contains("system probe foreground-window self-test"));
    assert!(rig.contains("[\"--self-test\"]"));
    assert!(rig.contains(
        "AX-focused/main identity wins over same-process app-share auxiliary compositor order"
    ));
}

#[test]
fn macos_packaged_evidence_proves_same_pid_sibling_routing_without_unsafe_negative() {
    let fixture = fs::read_to_string("evidence/v0.12.59/computer/HelperEvidenceFixture.swift")
        .unwrap()
        .replace("\r\n", "\n");
    let probe = fs::read_to_string("evidence/v0.12.59/computer/SystemProbe.swift")
        .unwrap()
        .replace("\r\n", "\n");
    let rig = fs::read_to_string("evidence/v0.12.59/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let readme = fs::read_to_string("evidence/v0.12.59/computer/README.md")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "private let siblingFixtureTitle = \"LBB v0.12.59 Same-PID Sibling Receiver\"",
        "var primaryWindowId = 0",
        "var siblingWindowId = 0",
        "var siblingTextLength = 0",
        "var siblingClicks = 0",
        "var siblingFocusCount = 0",
        "private final class SiblingView: NSView, NSTextFieldDelegate",
        "siblingField.setAccessibilityLabel(\"Sibling receiver sentinel\")",
        "let siblingWindow = NSWindow(",
        "siblingWindow.title = siblingFixtureTitle",
        "view.bindWindowTopology(",
        "primaryWindowId: window.windowNumber",
        "siblingWindowId: siblingWindow.windowNumber",
        "siblingWindow.orderFrontRegardless()",
    ] {
        assert!(
            fixture.contains(required),
            "fixture is missing genuine same-PID sibling topology: {required}"
        );
    }
    assert_eq!(fixture.matches("= FixtureWindow(").count(), 1);
    assert_eq!(fixture.matches("= NSWindow(").count(), 1);
    assert!(!fixture.contains("Process("));
    assert!(!fixture.contains("NSWorkspace.shared.launch"));

    let primary_preparation = fixture
        .split("func focusSemanticField(controlSequence: Int, siblingView: SiblingView) -> Bool")
        .nth(1)
        .unwrap()
        .split("override func draw")
        .next()
        .unwrap();
    for required in [
        "window.makeKey()",
        "window.makeFirstResponder(semanticField)",
        "siblingView.prepareAsFocusedSibling()",
        "state.siblingFocusCount += 1",
    ] {
        assert!(primary_preparation.contains(required));
    }
    for forbidden in ["NSApp.activate", "orderFront", "makeKeyAndOrderFront"] {
        assert!(
            !primary_preparation.contains(forbidden),
            "same-PID receiver preparation must not activate or raise: {forbidden}"
        );
    }

    for required in [
        "@_silgen_name(\"_AXUIElementGetWindow\")",
        "kAXFocusedWindowAttribute",
        "kAXFrontmostAttribute",
        "foregroundPIDBefore",
        "foregroundPIDAfter",
        "foregroundIdentityStable",
        "rawForegroundIdentityStable",
        "rawForegroundPID",
        "rawForegroundPSN",
        "foregroundAXMainWindowID",
        "targetFocusedWindowID",
        "targetMainWindowID",
        "targetAXFrontmost",
        "activeTargetObserved",
    ] {
        assert!(
            probe.contains(required),
            "system probe is missing read-only AX focus evidence: {required}"
        );
    }
    assert!(
        probe.contains("let waitMilliseconds = min(requestedWaitMilliseconds, 16_000)"),
        "the native active-receiver probe must cover the full server call boundary"
    );
    assert!(
        rig.contains("const ACTIVE_TARGET_RECEIVER_PROBE_TIMEOUT_MS = 16_000;"),
        "the packaged lane must retain the bounded active-receiver probe deadline"
    );
    assert!(
        rig.contains("Number(targetWindow.id),\n    ACTIVE_TARGET_RECEIVER_PROBE_TIMEOUT_MS,"),
        "the live receiver probe must use the shared bounded deadline"
    );
    let invariants = rig
        .split("function systemInvariants(before, after)")
        .nth(1)
        .unwrap()
        .split("function allIndependentInvariantsHeld")
        .next()
        .unwrap();
    for required in [
        "foregroundIdentitySandwichHeld",
        "rawForegroundIdentitySandwichHeld",
        "foregroundAXFocusedWindowID",
        "foregroundAXMainWindowID",
        "foregroundAxFocusUnchanged",
        "foregroundAXFrontmost",
        "foregroundAxFrontmostHeld",
        "before.frontWindowID === after.frontWindowID",
    ] {
        assert!(
            invariants.contains(required),
            "independent non-interruption oracle is missing: {required}"
        );
    }

    for required in [
        "const targetAfter = processProbe(systemProbeBinary, fixtureTargetPid)",
        "targetAfter.targetFocusedWindowID === fixtureSiblingWindowId",
        "targetAfter.targetMainWindowID === fixtureSiblingWindowId",
        "targetSiblingExpectedAfter: true",
        "targetSiblingExpectedAfter: false",
        "expectedFocusedAfter: targetSiblingExpectedAfter",
        "expectationMet",
    ] {
        assert!(
            rig.contains(required),
            "bounded same-PID failure diagnostics are missing: {required}"
        );
    }

    let discovery = rig
        .find("const siblingWindows = status.windows.filter")
        .unwrap();
    let first_observe = rig
        .find("let observed = await freshObserve(targetWindow.id)")
        .unwrap();
    let pointer_before = rig.find("const pixelSiblingFocusBefore").unwrap();
    let pointer_action = rig
        .find("const clickPromise = command(\"computer.click\"")
        .unwrap();
    let pointer_after = rig.find("const pixelSiblingFocusAfter").unwrap();
    let text_before = rig.find("const nativeTextReceiverBefore").unwrap();
    let text_action = rig
        .find("const nativeTextBody = await command(\"computer.typeText\"")
        .unwrap();
    let text_after = rig.find("const nativeTextReceiverAfter").unwrap();
    assert!(discovery < first_observe);
    assert!(pointer_before < pointer_action && pointer_action < pointer_after);
    assert!(text_before < text_action && text_action < text_after);

    for required in [
        "siblingWindows[0].pid === targetWindow.pid",
        "String(fixtureReady.primaryWindowId) === targetWindow.id",
        "String(fixtureReady.siblingWindowId) === siblingWindows[0].id",
        "exactTargetReceiverMatches(startupSiblingFocus, fixtureReady.siblingWindowId, false)",
        "processProbeWaitingForActive(",
        "activeTargetObserved === true",
        "exact active target receiver observed during pixel lease",
        "exactTargetReceiverMatches(activeRequestedReceiver, Number(targetWindow.id), true)",
        "same-PID sibling is remembered before primary pixel dispatch",
        "background pixel click targets only primary and restores sibling receiver",
        "clickedFixtureState.siblingClicks === pixelFixtureBefore.siblingClicks",
        "requireIndependentInvariants(\"same-PID pixel routing\", pixelSystemInvariants)",
        "same-PID sibling remains the exact prior receiver immediately before text request",
        "native typeText exact fixture read-back with zero sibling mutation",
        "nativeTextFixtureState.siblingTextLength === nativeTextFocusedState.siblingTextLength",
        "every retained screenshot is bound only to the primary exact window",
        "screenshot.windowId !== siblingWindow.id",
        "samePidMultiWindowRouting: {",
        "status: \"unproven-live\"",
        "attempted: false",
        "productionHookUsed: false",
        "focusRaceUsed: false",
    ] {
        assert!(
            rig.contains(required),
            "same-PID rig proof is missing: {required}"
        );
    }
    for forbidden in [
        "force-sibling-before-dispatch",
        "wrong-sibling-delay",
        "NSApp.activate",
        "makeKeyAndOrderFront",
        "LBB_COMPUTER_TEST",
    ] {
        assert!(
            !rig.contains(forbidden),
            "same-PID evidence uses a race, activation, or production hook: {forbidden}"
        );
    }

    for required in [
        "two clearly titled, visible, genuine",
        "same-PID sibling receiver sentinel",
        "restore the same sibling",
        "wrong-sibling-at-dispatch negative is explicitly **unproven**",
        "timing race or a",
        "requires all six manifest entries to name the primary window ID",
        "sibling field's content is never serialized",
        "diagnostics retain only availability, expected-focus, observed-focus,",
        "never the target PID or either native window ID",
    ] {
        assert!(
            readme.contains(required),
            "same-PID evidence README is missing: {required}"
        );
    }
}

#[test]
fn windows_live_runner_causally_proves_cancel_quarantine_and_recovery() {
    let runner = fs::read_to_string("scripts/test-windows-computer-use.ps1")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "\"Cancellation\", \"All\"",
        "@(\"Smoke\", \"Recovery\", \"Semantic\", \"Keyboard\", \"Pixel\", \"Capture\", \"Cancellation\")",
        "Start-LbbCommandRequest \"computer.move\" $cancelParams $cancelCallId",
        "target-routed WM_MOUSEMOVE before explicit cancellation",
        "CALL_IN_PROGRESS",
        "Invoke-LbbCancelResponse $cancelCallId",
        "COMMAND_OUTCOME_UNKNOWN",
        "CALL_NOT_IN_PROGRESS",
        "$replayedCanceled.body.replayed -eq $true",
        "CALL_ID_REUSED",
        "NO_COMPUTER_SCREENSHOT",
        "a replacement Windows helper worker after outcome-unknown cancellation",
        "$replacementSessionId -ne $cancellationSessionId",
        "$cancellationReplacementWorkerPid -ne $cancellationWorkerPid",
        "$helperProcess.Id -eq $cancellationSupervisorPid",
        "$serverProcess.Id -eq $cancellationServerPid",
        "An old worker or queued native frame replaced the ready helper or republished old-session authority after cancellation",
        "NO_COMPUTER_FRAME",
        "coordinateSpace = \"normalized1000\"",
        "The replacement helper accepted normalized coordinates before an explicit observation supplied frame dimensions",
        "explicit cancellation replacement has no frame before observe",
        "replacementNoFrameCode",
        "computer.observe",
        "$recoveryFrame.frameId -ne $canceledFrameId",
        "COMPUTER_STALE_FRAME",
        "a fresh post-cancellation WM_MOUSEMOVE",
        "Explicit cancellation and recovery",
        "Close-LbbPendingRequest $pendingCancellationRequest",
    ] {
        assert!(
            runner.contains(required),
            "Windows runner is missing live cancellation proof: {required}"
        );
    }

    for forbidden in [
        "mock cancellation",
        "LBB_COMPUTER_TEST",
        "CANCEL_AFTER_START_MS",
        "Start-Sleep -Milliseconds 2000",
        "Stop-Process -Name",
        "explicit cancellation pre-recovery gate",
        "preRecoveryGateCode",
        "oldSessionRevoked",
    ] {
        assert!(
            !runner.contains(forbidden),
            "Windows cancellation proof uses an unsafe or synthetic shortcut: {forbidden}"
        );
    }
}

#[test]
fn windows_live_runner_is_bound_to_one_frozen_release_candidate() {
    let runner = fs::read_to_string("scripts/test-windows-computer-use.ps1")
        .unwrap()
        .replace("\r\n", "\n");
    let packaged_verifier = fs::read_to_string("scripts/verify-windows-artifacts.ps1")
        .unwrap()
        .replace("\r\n", "\n");
    let candidate_workflow = fs::read_to_string(".github/workflows/deploy.yml").unwrap();
    let development = fs::read_to_string("docs/DEVELOPMENT.md").unwrap();

    for required in [
        "[string]$Version",
        "[string]$ChecksumManifest",
        "[string]$ChecksumManifestSha256",
        "[string]$CandidateBindingPath",
        "ChecksumManifest must use the canonical SHA256SUMS.txt filename",
        "externally recorded frozen-candidate SHA-256",
        "Read-ExactCandidateChecksums",
        "exactly the four canonical release assets",
        "local-browser-bridge-v$Version-windows-x86_64.exe",
        "local-computer-helper-v$Version-windows-x86_64.exe",
        "local-browser-bridge-v$Version-macos-universal.tar.gz",
        "local-browser-bridge-extension-v$Version.zip",
        "Get-VerifiedCandidateArtifact",
        "does not match its exact ChecksumManifest entry",
        "$versionInfo.FileVersion -cne $Version",
        "$versionInfo.ProductVersion -cne $Version",
        "Get-BoundedReportedVersion",
        "-ExpectedReportedVersion \"local-browser-bridge-desktop $Version\"",
        "local-computer-helper $Version",
        "checksumManifestMatched = $true",
        "exactAssetSetMatched = $true",
        "candidateBinding = $candidateBinding",
        "Read-ExactReleaseCandidateBinding",
        "releaseCandidateBinding = $releaseCandidateBinding",
        "CandidateBindingPath does not bind the exact frozen workflow candidate",
        "schemaVersion = 2",
    ] {
        assert!(
            runner.contains(required),
            "Windows candidate binding is missing {required}"
        );
    }
    assert!(
        packaged_verifier.contains("ExpectedVersion = \"local-browser-bridge-desktop $Version\"")
    );
    assert!(candidate_workflow.contains(
        "Copy-Item target/x86_64-pc-windows-msvc/release/local-browser-bridge-desktop.exe $server"
    ));
    assert!(development.contains("out-of-band binding value"));
    assert!(development.contains("candidateBinding.checksumManifestMatched: true"));
    assert!(development.contains("releaseCandidateBinding"));

    let candidate_binding = runner.find("$candidateBinding = [ordered]@{").unwrap();
    let evidence_creation = runner
        .find("[IO.Directory]::CreateDirectory($evidenceRoot)")
        .unwrap();
    let first_process_launch = runner.find("Start-IsolatedProcess $hostPath").unwrap();
    assert!(candidate_binding < evidence_creation);
    assert!(candidate_binding < first_process_launch);
}

#[test]
fn windows_helper_readiness_is_protocol_and_process_bound() {
    let runner = fs::read_to_string("scripts/test-windows-computer-use.ps1")
        .unwrap()
        .replace("\r\n", "\n");
    let helper = fs::read_to_string("src/bin/local-computer-helper.rs").unwrap();
    let server = fs::read_to_string("src/server.rs").unwrap();
    let ci = fs::read_to_string(".github/workflows/ci.yml").unwrap();
    let deploy = fs::read_to_string(".github/workflows/deploy.yml").unwrap();

    for required in [
        "QueryFullProcessImageName",
        "ProcessIdToSessionId",
        "GetDirectChildProcessIds($SupervisorProcess.Id, $resolvedHelper)",
        "function Test-BoundHelperWorkerCandidate {",
        "$reportedControllerPid -eq $SupervisorProcess.Id",
        "$script:nativeProbeType::ProcessImageMatches(",
        "$children.Count -eq 1 -and [int64]$children[0] -ne $reportedWorkerPid",
        "The enumerated exact-image helper child did not match the authenticated worker process.",
        "$stablePolls -ge 2",
        "controllerPid = [int]$reportedControllerPid",
        "directChildEnumerated = $children.Count -eq 1",
        "The authenticated helper binding rejected an exact live worker when Toolhelp returned no child.",
        "The authenticated helper binding accepted a conflicting Toolhelp child.",
        "exactImageMatched = $true",
        "helloStateMatched = $true",
        "protocolRoundTrip = $false",
        "Complete-HelperTopologyRoundTrip",
        "helperTopologyTransitionCount",
        "helperTopologyHistory",
        "stage = $Stage",
        "observedAtUtc = [DateTime]::UtcNow.ToString(\"o\")",
        "helperTopologyLastObservation",
        "The helper supervisor exited with code",
        "protocol-bound helper readiness",
    ] {
        assert!(
            runner.contains(required),
            "Windows helper readiness proof is missing: {required}"
        );
    }

    let direct_child_probe = runner
        .split("public static int[] GetDirectChildProcessIds(int parentProcessId, string expectedImagePath)")
        .nth(1)
        .unwrap()
        .split("public static int GetProcessSessionId(int processId)")
        .next()
        .unwrap();
    assert!(direct_child_probe.contains("entry.ParentProcessId == (uint)parentProcessId"));
    assert!(direct_child_probe.contains("ReadProcessImagePath(entry.ProcessId)"));
    assert!(direct_child_probe.contains(
        "String.Equals(Path.GetFullPath(childPath), expectedPath, StringComparison.OrdinalIgnoreCase)"
    ));
    assert!(
        !direct_child_probe.contains("String.Equals(entry.ExeFile"),
        "Toolhelp's basename is advisory and must not prefilter exact full-image verification"
    );

    let generic_wait = runner.split("function Wait-Condition {").nth(1).unwrap();
    let generic_wait = generic_wait
        .split("function Wait-ForFixtureProof {")
        .next()
        .unwrap();
    assert!(generic_wait.contains("[scriptblock]$Probe"));
    assert!(generic_wait.contains("$value = & $Probe"));
    assert!(!generic_wait.contains("[scriptblock]$Condition"));
    assert!(
        !generic_wait.contains("catch"),
        "generic readiness waits must not erase fatal process or Win32 errors"
    );

    assert!(helper.contains("shutdown_signal(tokio::signal::ctrl_c())"));
    assert!(helper.contains("std::future::pending::<()>().await"));
    assert!(helper.contains("object.insert(\"processId\".to_owned(), json!(std::process::id()))"));
    assert!(helper.contains("object.insert(\n            \"controllerProcessId\".to_owned(),"));
    assert!(server.contains("&& process_id.is_some()"));
    assert!(server.contains("&& controller_process_id.is_some()"));
    assert!(server.contains("process_id: process_id.unwrap_or(0)"));
    assert!(server.contains("controller_process_id: controller_process_id.unwrap_or(0)"));
    assert!(ci.contains("scripts/test-windows-computer-use.ps1"));
    assert!(
        !deploy.contains("scripts/test-windows-computer-use.ps1"),
        "the candidate workflow must reuse reviewed CI instead of rerunning the native acceptance self-tests"
    );
    let coordinator_self_test = "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/run-windows-computer-use-acceptance.ps1 -Mode SelfTest";
    assert!(ci.contains(coordinator_self_test));
    assert_eq!(ci.matches(coordinator_self_test).count(), 1);
    assert!(
        !ci.contains("test-windows-computer-use.ps1 -SelfTest"),
        "CI must exercise the Job-sensitive runner only through the topology-aware coordinator self-test"
    );
    assert!(ci.contains(
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./tests/fixtures/windows/WindowsComputerUseFixture.ps1 -SelfTest"
    ));
    assert!(
        !ci.contains("& ./tests/fixtures/windows/WindowsComputerUseFixture.ps1 -SelfTest"),
        "the .NET Framework fixture must be parsed by PowerShell Core but executed by Windows PowerShell 5.1"
    );
    for required in [
        "$selfTestJob.StartProcess(",
        "GetDirectChildProcessIds($PID, $selfTestHostPath)",
        "local-computer-helper-v0.12.59-windows-x86_64.exe",
        "$renamedHelperSelfTestJob.StartProcess(",
        "GetDirectChildProcessIds($PID, $renamedHelperSelfTestPath)",
        "The native exact-parent/full-image probe did not identify the renamed packaged-image self-test child.",
        "$selfTestActiveProcessCount -lt 1",
        "$selfTestJob.Terminate()",
        "$selfTestJob.ActiveProcessCount -ne 0",
    ] {
        assert!(
            runner.contains(required),
            "Windows acceptance self-test is missing: {required}"
        );
    }
}

#[test]
fn windows_owned_process_retains_the_create_process_handle_without_pid_reopen() {
    let runner = fs::read_to_string("scripts/test-windows-computer-use.ps1")
        .unwrap()
        .replace("\r\n", "\n");

    let owned_handle = runner
        .split("public sealed class OwnedProcessHandle : IDisposable")
        .nth(1)
        .unwrap()
        .split("public sealed class OwnedProcessJob : IDisposable")
        .next()
        .unwrap();
    for required in [
        "internal OwnedProcessHandle(IntPtr processHandle, int processId)",
        "public int Id { get; private set; }",
        "public bool HasExited",
        "public bool WaitForExit(int milliseconds)",
        "public void Refresh()",
        "public int ExitCode",
        "GetExitCodeProcess(handle, out exitCode)",
        "public void Dispose()",
        "if (ownedHandle != IntPtr.Zero && !CloseHandle(ownedHandle) && disposing)",
        "throw new ObjectDisposedException(\"OwnedProcessHandle\")",
    ] {
        assert!(
            owned_handle.contains(required),
            "the exact owned process-handle wrapper is missing: {required}"
        );
    }
    assert!(
        !owned_handle.contains("OpenProcess(") && !owned_handle.contains("GetProcessById"),
        "the exact owned process wrapper must never reacquire a process by PID"
    );

    let owned_job = runner
        .split("public sealed class OwnedProcessJob : IDisposable")
        .nth(1)
        .unwrap()
        .split("$script:ownedProcessType =")
        .next()
        .unwrap();
    for required in [
        "public OwnedProcessHandle StartProcess(",
        "OwnedProcessHandle ownedProcess = new OwnedProcessHandle(process.Process, process.ProcessId);",
        "process.Process = IntPtr.Zero;",
        "return ownedProcess;",
        "if (process.Process != IntPtr.Zero) { CloseHandle(process.Process); }",
    ] {
        assert!(
            owned_job.contains(required),
            "the Job launcher exact-handle transfer is missing: {required}"
        );
    }
    let resume = owned_job.find("ResumeThread(process.Thread)").unwrap();
    let wrap = owned_job
        .find("OwnedProcessHandle ownedProcess = new OwnedProcessHandle(")
        .unwrap();
    let transfer = owned_job.find("process.Process = IntPtr.Zero;").unwrap();
    let resume_complete = transfer
        + owned_job[transfer..]
            .find("suspendedProcessNeedsTermination = false;")
            .unwrap();
    let returned = resume_complete
        + owned_job[resume_complete..]
            .find("return ownedProcess;")
            .unwrap();
    assert!(
        resume < wrap
            && wrap < transfer
            && transfer < resume_complete
            && resume_complete < returned
    );
    assert!(!owned_job.contains("return process.ProcessId;"));

    let isolated_start = runner
        .split("function Start-IsolatedProcess {")
        .nth(1)
        .unwrap()
        .split("function Request-FixtureStop {")
        .next()
        .unwrap();
    assert!(isolated_start.contains("return $script:ownedJob.StartProcess("));
    assert!(!isolated_start.contains("GetProcessById"));
    assert!(
        !runner.contains("[Diagnostics.Process]::GetProcessById("),
        "runner-owned processes must never be rebound through a PID lookup"
    );

    let self_test = runner
        .split("if ($SelfTest) {")
        .nth(1)
        .unwrap()
        .split("Write-Output \"Windows computer-use acceptance self-test passed.\"")
        .next()
        .unwrap();
    for required in [
        "$selfTestChild = $selfTestJob.StartProcess(",
        "$selfTestChild.GetType() -ne $script:ownedProcessType",
        "$selfTestChildPid = $selfTestChild.Id",
        "$selfTestChild.WaitForExit(5000)",
        "$selfTestChild.ExitCode -ne 1",
        "$fixtureBuildLaunchSelfTestBuilder = $fixtureBuildLaunchSelfTestJob.StartProcess(",
        "$fixtureBuildLaunchSelfTestExecutableProcess = $fixtureBuildLaunchSelfTestJob.StartProcess(",
    ] {
        assert!(
            self_test.contains(required),
            "the Windows exact-handle self-test is missing: {required}"
        );
    }

    for required in [
        "$helperProcess.Dispose()",
        "$serverProcess.Dispose()",
        "$fixtureProcess.Dispose()",
        "$fixtureBuilderProcess.Dispose()",
        "$fixtureExecutableSelfTestProcess.Dispose()",
    ] {
        assert!(
            runner.contains(required),
            "the runner does not release an exact owned process handle: {required}"
        );
    }
}

#[test]
fn windows_fixture_wait_avoids_powershell_dynamic_scope_recursion() {
    let runner = fs::read_to_string("scripts/test-windows-computer-use.ps1")
        .unwrap()
        .replace("\r\n", "\n");

    let fixture_wait = runner
        .split("function Wait-ForFixtureProof {")
        .nth(1)
        .unwrap()
        .split("function Write-NewOperatorMarker {")
        .next()
        .unwrap();
    for required in [
        "[scriptblock]$FixturePredicate",
        "[scriptblock]$StateReader = { Get-FixtureState }",
        "$state = & $StateReader",
        "if (& $FixturePredicate $state)",
        "$timeoutWatch = [Diagnostics.Stopwatch]::StartNew()",
        "$timeoutWatch.ElapsedMilliseconds -lt $TimeoutMilliseconds",
        "Timed out waiting for $Description.",
    ] {
        assert!(
            fixture_wait.contains(required),
            "Windows fixture wait is missing its non-recursive contract: {required}"
        );
    }
    assert!(
        !fixture_wait.contains("return Wait-Condition")
            && !fixture_wait.contains("Wait-Condition {")
            && !fixture_wait.contains("[scriptblock]$Condition")
            && !fixture_wait.contains("[DateTime]::UtcNow"),
        "the fixture wait must not reintroduce dynamic-scope predicate shadowing"
    );

    for required in [
        "the synthetic false-false-true fixture state",
        "$fixtureWaitProbe.stateReads -ne 3",
        "$fixtureWaitProbe.predicateCalls -ne 3",
        "the bounded synthetic fixture timeout",
        "$fixtureTimeoutWatch.ElapsedMilliseconds -gt 2000",
        "synthetic-fixture-predicate-failure",
        "The fixture wait did not propagate its predicate failure unchanged.",
    ] {
        assert!(
            runner.contains(required),
            "Windows fixture wait self-test is missing: {required}"
        );
    }

    for required in [
        "$script:runStage = \"wait-foreground-arm\"",
        "failureDetails = [ordered]@{",
        "stage = $script:runStage",
        "fullyQualifiedErrorId = ConvertTo-SafeFailureText",
        "scriptStackTrace = ConvertTo-SafeFailureText",
        "@($PSCommandPath, \"[RUNNER]\")",
        "pathsRecorded = $false",
    ] {
        assert!(
            runner.contains(required),
            "Windows failure diagnostics are missing: {required}"
        );
    }
}

#[test]
fn windows_fixture_is_a_source_bound_dedicated_gui_process_with_exact_cleanup() {
    let runner = fs::read_to_string("scripts/test-windows-computer-use.ps1")
        .unwrap()
        .replace("\r\n", "\n");
    let fixture = fs::read_to_string("tests/fixtures/windows/WindowsComputerUseFixture.ps1")
        .unwrap()
        .replace("\r\n", "\n");
    let verifier = fs::read_to_string("scripts/verify-release-acceptance-evidence.sh").unwrap();

    for required in [
        "[Parameter(ParameterSetName = \"Build\", Mandatory = $true)]",
        "[ValidatePattern('^[0-9a-f]{64}$')]",
        "[string]$ExpectedSourceSha256",
        "private const string AppUserModelId = \"LocalBrowserBridge.WindowsAcceptance\";",
        "[DllImport(\"shell32.dll\", CharSet = CharSet.Unicode)]",
        "private static extern int SetCurrentProcessExplicitAppUserModelID(string appId);",
        "[STAThread]",
        "private static int Main(string[] args)",
        "selfTest = args != null && args.Length == 1 &&",
        "String.Equals(args[0], \"--self-test\", StringComparison.Ordinal);",
        "args == null || (args.Length != 2 && args.Length != 3)",
        "!String.Equals(args[0], \"--evidence-directory\", StringComparison.Ordinal)",
        "String.IsNullOrWhiteSpace(args[1])",
        "!IsDriveAbsoluteNonRootPath(args[1])",
        "!String.Equals(args[2], \"--show-occluder\", StringComparison.Ordinal)",
        "private static bool IsDriveAbsoluteNonRootPath(string value)",
        "!Char.IsLetter(value[0]) || value[1] != ':'",
        "string root = Path.GetPathRoot(fullPath);",
        "TryParseArguments(new string[] { \"--SELF-TEST\" }",
        "TryParseArguments(new string[] { \"--evidence-directory\", \"relative\" }",
        "TryParseArguments(new string[] { \"--evidence-directory\", \"C:drive-relative\" }",
        r#"TryParseArguments(new string[] { "--evidence-directory", "C:\\" }"#,
        r#"TryParseArguments(new string[] { "--evidence-directory", "\\root-relative" }"#,
        r#"TryParseArguments(new string[] { "--evidence-directory", "\\\\server\\share\\evidence" }"#,
        "TryParseArguments(new string[] { \"--evidence-directory\", absoluteEvidence, \"--SHOW-OCCLUDER\" }",
        "if (!selfTest && !IsFreshEvidenceDirectory(evidenceDirectory))",
        "private static bool PathEntryExists(string path)",
        "File.GetAttributes(path);",
        "private static bool IsFreshEvidenceDirectory(string evidenceDirectory)",
        "(attributes & FileAttributes.ReparsePoint) != 0",
        "PathEntryExists(Path.Combine(evidenceDirectory, protectedName))",
        "\"fixture-state.json\"",
        "\"fixture-events.ndjson\"",
        "\"fixture-ready.json\"",
        "int appIdResult = SetCurrentProcessExplicitAppUserModelID(AppUserModelId);",
        "$sourceSha256BeforeBuild = Get-FixtureSourceSha256 $PSCommandPath",
        "$sourceSha256BeforeBuild -cne $ExpectedSourceSha256",
        "$sourceSha256AfterBuild = Get-FixtureSourceSha256 $PSCommandPath",
        "$sourceSha256AfterBuild -cne $ExpectedSourceSha256",
        "-OutputAssembly $outputPath",
        "-OutputType WindowsApplication",
        "Write-Output \"Windows computer-use fixture executable built.\"",
    ] {
        assert!(
            fixture.contains(required),
            "dedicated Windows fixture contract is missing: {required}"
        );
    }
    let argument_parser = fixture
        .split("private static bool TryParseArguments(")
        .nth(1)
        .unwrap()
        .split("private static bool IsDriveAbsoluteNonRootPath(string value)")
        .next()
        .unwrap();
    assert!(
        !argument_parser.contains("OrdinalIgnoreCase")
            && !argument_parser.contains("StartsWith")
            && !argument_parser.contains("Contains("),
        "the dedicated fixture CLI must accept only the exact ordinal grammar"
    );
    let build_mode = fixture
        .split("if ($PSCmdlet.ParameterSetName -eq \"Build\") {")
        .nth(1)
        .unwrap()
        .split("Add-Type -TypeDefinition $fixtureSource")
        .next()
        .unwrap();
    assert!(!build_mode.contains("ConsoleApplication"));

    for required in [
        "$fixtureBuildDirectory = [IO.Path]::Combine(",
        "\"lbb-windows-computer-use-fixture-\" + [Guid]::NewGuid().ToString(\"N\")",
        "executionMode = \"dedicated-windows-application\"",
        "appUserModelId = \"LocalBrowserBridge.WindowsAcceptance\"",
        "sourceScriptSha256 = (Get-FileSha256 $resolvedFixture)",
        "sourceStableAcrossBuild = $false",
        "executableStableAcrossLaunch = $false",
        "entryPointSelfTestPassed = $false",
        "directChildMatched = $false",
        "exactImageMatched = $false",
        "interactiveSessionMatched = $false",
        "readyPidMatched = $false",
        "executableRemoved = $false",
        "terminalHostUsed = $false",
        "pathsRecorded = $false",
        "$fixtureBuilderProcess = Start-IsolatedProcess $hostPath $fixtureBuilderArguments @{}",
        "\"-BuildExecutablePath\"",
        "\"-ExpectedSourceSha256\"",
        "$fixtureProcessBinding.sourceScriptSha256",
        "$fixtureBuildAttributes = [IO.File]::GetAttributes($fixtureBuildDirectory)",
        "$fixtureProcessBinding.executableBytes -gt 0 -and $fixtureProcessBinding.executableBytes -le 20971520",
        "$fixtureExecutableSelfTestProcess = Start-IsolatedProcess $fixtureExecutablePath @(\"--self-test\") @{}",
        "$fixtureProcessBinding.entryPointSelfTestPassed = $true",
        "(Get-FileSha256 $fixtureExecutablePath) -ceq $fixtureProcessBinding.executableSha256",
        "The dedicated Windows fixture executable changed during its entry-point self-test.",
        "private const uint CREATE_BREAKAWAY_FROM_JOB = 0x01000000;",
        "private const uint PROC_THREAD_ATTRIBUTE_JOB_LIST = 0x0002000D;",
        "Could not atomically bind the child to the private acceptance-test Job Object",
        "The suspended child was not atomically assigned to the private acceptance-test Job Object",
        "$fixtureBuildLaunchSelfTestJob = $script:ownedJobType::new()",
        "$fixtureBuildLaunchSelfTestBuilder = $fixtureBuildLaunchSelfTestJob.StartProcess(",
        "The Job-owned source-bound fixture build self-test failed.",
        "The Job-owned dedicated fixture entry-point self-test failed.",
        "$fixtureBuildLaunchSelfTestEnabled = (",
        "$PSVersionTable.PSEdition -ceq \"Desktop\"",
        "if ($fixtureBuildLaunchSelfTestEnabled) {",
        "$fixtureArguments = @(\"--evidence-directory\", $fixtureEvidence)",
        "$fixtureArguments += \"--show-occluder\"",
        "$fixtureProcess = Start-IsolatedProcess $fixtureExecutablePath $fixtureArguments @{}",
        "$script:nativeProbeType::ProcessImageMatches(",
        "$script:nativeProbeType::GetProcessSessionId($fixtureProcess.Id) -eq $sessionId",
        "$fixtureProcessBinding.executableStableAcrossLaunch = (",
        "The dedicated Windows fixture executable changed before its live image binding completed.",
        "$targetPid = [int]$script:fixtureReady.processId",
        "$fixtureProcessBinding.readyPidMatched = $targetPid -eq $fixtureProcess.Id",
        "$script:nativeProbeType::GetDirectChildProcessIds($PID, $fixtureExecutablePath)",
        "$fixtureDirectChildren.Count -eq 1",
        "[int]$fixtureDirectChildren[0] -eq $fixtureProcess.Id",
        "fixtureProcessBinding = $fixtureProcessBinding",
    ] {
        assert!(
            runner.contains(required),
            "dedicated Windows fixture runner contract is missing: {required}"
        );
    }
    let owned_job = runner
        .split("public sealed class OwnedProcessJob : IDisposable")
        .nth(1)
        .unwrap()
        .split("$script:ownedJobType =")
        .next()
        .unwrap();
    let handle_list = owned_job
        .find("new UIntPtr(PROC_THREAD_ATTRIBUTE_HANDLE_LIST)")
        .unwrap();
    let job_list = owned_job
        .find("new UIntPtr(PROC_THREAD_ATTRIBUTE_JOB_LIST)")
        .unwrap();
    let create = owned_job.find("bool created = CreateProcess(").unwrap();
    let verify = owned_job
        .find("IsProcessInJob(process.Process, handle, out assignedToPrivateJob)")
        .unwrap();
    let resume = owned_job.find("ResumeThread(process.Thread)").unwrap();
    assert!(handle_list < job_list && job_list < create && create < verify && verify < resume);
    assert!(
        owned_job
            .contains("CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | CREATE_BREAKAWAY_FROM_JOB")
    );
    assert!(!owned_job.contains("AssignProcessToJobObject("));
    assert!(owned_job.contains("JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE"));
    assert!(!owned_job.contains("JOB_OBJECT_LIMIT_BREAKAWAY_OK"));
    assert!(!owned_job.contains("JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK"));
    let desktop_fixture_probe = runner
        .split("if ($fixtureBuildLaunchSelfTestEnabled) {")
        .nth(1)
        .unwrap()
        .split("$fixtureWaitProbe = [ordered]@{")
        .next()
        .unwrap();
    assert!(desktop_fixture_probe.contains("-BuildExecutablePath"));
    assert!(desktop_fixture_probe.contains("$fixtureBuildLaunchSelfTestJob.StartProcess("));
    for required in [
        "public static int[] GetDirectChildProcessIds(int parentProcessId, string expectedImagePath)",
        "entry.ParentProcessId == (uint)parentProcessId",
        "String.Equals(Path.GetFullPath(childPath), expectedPath, StringComparison.OrdinalIgnoreCase)",
        "public static bool ProcessImageMatches(int processId, string expectedImagePath)",
        "Path.GetFullPath(observed)",
        "Path.GetFullPath(expectedImagePath)",
    ] {
        assert!(
            runner.contains(required),
            "native fixture process binding is missing: {required}"
        );
    }
    let build = runner
        .find("$script:runStage = \"build-dedicated-fixture\"")
        .unwrap();
    let executable_self_test = runner
        .find("$script:runStage = \"self-test-dedicated-fixture\"")
        .unwrap();
    let start = runner
        .find("$script:runStage = \"start-dedicated-fixture\"")
        .unwrap();
    let ready_pid = runner
        .find("$fixtureProcessBinding.readyPidMatched = $targetPid -eq $fixtureProcess.Id")
        .unwrap();
    let direct_child = runner
        .find("$fixtureProcessBinding.directChildMatched = (")
        .unwrap();
    let server = runner
        .find("$script:runStage = \"start-loopback-server\"")
        .unwrap();
    assert!(build < executable_self_test);
    assert!(executable_self_test < start);
    assert!(start < ready_pid);
    assert!(ready_pid < direct_child);
    assert!(direct_child < server);
    assert!(
        !runner[start..server].contains("Start-IsolatedProcess $hostPath $fixtureArguments"),
        "the live GUI fixture must execute directly rather than through a PowerShell console host"
    );

    let exact_cleanup = runner
        .split("function Remove-OwnedFixtureBuildDirectory {")
        .nth(1)
        .unwrap()
        .split("\n}\n\nif ($SelfTest) {")
        .next()
        .unwrap();
    for required in [
        "$resolvedPath = [IO.Path]::GetFullPath($Path)",
        "$resolvedExecutable = [IO.Path]::GetFullPath($ExpectedExecutable)",
        "[IO.Path]::GetDirectoryName($resolvedExecutable)",
        "$resolvedPath,",
        "[StringComparison]::OrdinalIgnoreCase",
        "[IO.File]::GetAttributes($resolvedPath)",
        "catch [IO.FileNotFoundException]",
        "catch [IO.DirectoryNotFoundException]",
        "($directoryAttributes -band [IO.FileAttributes]::Directory) -eq 0",
        "[IO.FileAttributes]::ReparsePoint",
        "The runner-owned dedicated fixture build directory is not an ordinary directory.",
        "$entries = @([IO.Directory]::EnumerateFileSystemEntries($resolvedPath))",
        "$entries.Count -gt 1",
        "The runner-owned dedicated fixture build directory contains an unexpected entry.",
        "The runner-owned dedicated fixture executable is not an ordinary file.",
        "$deadline = [DateTime]::UtcNow.AddSeconds(5)",
        "[IO.File]::Delete($resolvedExecutable)",
        "[IO.Directory]::Delete($resolvedPath, $false)",
    ] {
        assert!(
            exact_cleanup.contains(required),
            "exact dedicated fixture cleanup is missing: {required}"
        );
    }
    for forbidden in [
        "[IO.Directory]::Delete($resolvedPath, $true)",
        "Remove-Item",
        "SearchOption]::AllDirectories",
        "EnumerateFiles(",
    ] {
        assert!(
            !exact_cleanup.contains(forbidden),
            "dedicated fixture cleanup must not recurse or broaden its target: {forbidden}"
        );
    }
    let terminate_owned_job = runner.rfind("$script:ownedJob.Terminate()").unwrap();
    let dispose_fixture = runner
        .rfind("$fixtureProcess.Dispose()")
        .expect("fixture process handle must be disposed");
    let remove_executable = runner
        .rfind("Remove-OwnedFixtureBuildDirectory $fixtureBuildDirectory $fixtureExecutablePath")
        .unwrap();
    let record_removed = runner
        .rfind("$fixtureProcessBinding.executableRemoved = $true")
        .unwrap();
    let write_summary = runner
        .rfind("fixtureProcessBinding = $fixtureProcessBinding")
        .unwrap();
    assert!(terminate_owned_job < dispose_fixture);
    assert!(dispose_fixture < remove_executable);
    assert!(remove_executable < record_removed);
    assert!(record_removed < write_summary);

    for required in [
        "fixture_source_sha256=\"$(sha256_file tests/fixtures/windows/WindowsComputerUseFixture.ps1)\"",
        "--arg fixture_source_sha256 \"$fixture_source_sha256\"",
        ".fixtureProcessBinding.executionMode == \"dedicated-windows-application\"",
        ".fixtureProcessBinding.appUserModelId == \"LocalBrowserBridge.WindowsAcceptance\"",
        ".fixtureProcessBinding.sourceScriptSha256 == $fixture_source_sha256",
        ".fixtureProcessBinding.sourceStableAcrossBuild == true",
        ".fixtureProcessBinding.executableBytes | type == \"number\" and . > 0 and . <= 20971520",
        ".fixtureProcessBinding.executableSha256 | type == \"string\" and test(\"^[0-9a-f]{64}$\")",
        ".fixtureProcessBinding.executableStableAcrossLaunch == true",
        ".fixtureProcessBinding.entryPointSelfTestPassed == true",
        ".fixtureProcessBinding.directChildMatched == true",
        ".fixtureProcessBinding.exactImageMatched == true",
        ".fixtureProcessBinding.interactiveSessionMatched == true",
        ".fixtureProcessBinding.readyPidMatched == true",
        ".fixtureProcessBinding.executableRemoved == true",
        ".fixtureProcessBinding.terminalHostUsed == false",
        ".fixtureProcessBinding.pathsRecorded == false",
    ] {
        assert!(
            verifier.contains(required),
            "release verifier does not require the dedicated fixture fact: {required}"
        );
    }
    assert!(verifier.contains("and (.steps | type == \"array\" and length == 62)"));
    assert_eq!(runner.matches("Save-StepResponse \"").count(), 36);
    assert_eq!(runner.matches("Save-StepRecord \"").count(), 26);
    assert_eq!(runner.matches("Save-ObservationScreenshot $").count(), 18);
    assert_eq!(runner.matches("Save-SanitizedDesktopCrop \"").count(), 2);
}

#[test]
fn windows_foreground_arm_requires_fresh_mouse_ack_and_stable_native_samples() {
    let runner = fs::read_to_string("scripts/test-windows-computer-use.ps1")
        .unwrap()
        .replace("\r\n", "\n");
    let fixture = fs::read_to_string("tests/fixtures/windows/WindowsComputerUseFixture.ps1")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "[int]$ForegroundArmTimeoutSeconds = 300",
        "$PSVersionTable.PSEdition -cne \"Desktop\"",
        "$PSVersionTable.PSVersion.Major -ne 5",
        "Live Windows acceptance requires Windows PowerShell 5.1 (Desktop edition)",
        "$foregroundArmMessage = 0x8126",
        "$script:nativeProbeType::PostMessage(",
        "[IntPtr]$foregroundArmRequestGeneration",
        "ACTION REQUIRED: Through a separately authorized Windows Computer Use app share",
        "function Test-ForegroundArmRequestDeliveryState {",
        "$inputNotStarted -or $inputAlreadyComplete",
        "$script:runStage = \"wait-foreground-arm-request-delivery\"",
        "Save-StepRecord \"foreground arm request delivery\"",
        "foregroundArmButtonEnabled",
        "function Wait-ForStableForegroundArm {",
        "[scriptblock]$StateReader = { Get-FixtureStateSnapshot }",
        "[scriptblock]$NativeReader = { $script:nativeProbeType::Capture() }",
        "[int]$ExpectedFixtureProcessId",
        "[int]$RequestDeliveryTimeoutSeconds = 10",
        "[Int64]$AfterPublicationGeneration = 0",
        "requestDeliveryTimeoutSeconds = $RequestDeliveryTimeoutSeconds",
        "-RequestDeliveryTimeoutSeconds $foregroundArmRequestDeliveryTimeoutSeconds",
        "-AfterPublicationGeneration ([Int64]$armRequestDelivery.statePublicationGeneration)",
        "ValidateFixtureArmTopology",
        "GetWindowThreadProcessIdWithOwner",
        "IsWindow(IntPtr window)",
        "IsChild(IntPtr parent, IntPtr child)",
        "GetAncestor(IntPtr window, uint flags)",
        "[int]$RequiredStableSamples = 3",
        "$requestMatched = [int]$state.foregroundArmRequestedGeneration -eq $RequestedGeneration",
        "$acknowledgementMatched = [int]$state.foregroundArmAcknowledgedGeneration -eq $RequestedGeneration",
        "$foregroundMatched = [Int64]$native.ForegroundHwnd -eq $SentinelHwnd",
        "$focusMatched = [Int64]$native.FocusHwnd -eq $ArmButtonHwnd",
        "$cursorAvailable = $native.CursorAvailable -eq $true",
        "$leftMouseDownCountMatched",
        "$leftMouseUpCountMatched",
        "$nativeTopologyMatched",
        "$statePublicationAdvanced",
        "$statePublicationGeneration -eq $lastAcceptedPublicationGeneration",
        "A repeated read of the same valid publication is neutral",
        "$signature -ceq $previousSignature",
        "$timeoutWatch = [Diagnostics.Stopwatch]::StartNew()",
        "foregroundStable = $stableSamples -ge $RequiredStableSamples",
        "focusStable = $stableSamples -ge $RequiredStableSamples",
        "stableSamplesObserved = $stableSamples",
        "stablePublicationSamplesObserved = $stableSamples",
        "baselineContinuityMatched = $false",
        "rawWindowHandlesRecorded = $false",
        "rawCursorCoordinatesRecorded = $false",
        "snapshot resets arm stability instead of extending the arm timeout.",
        "Timed out waiting for a fresh foreground-arm click and $RequiredStableSamples stable native samples.",
        "Save-StepRecord \"foreground arm proof\" $script:foregroundArmProof",
        "foregroundArmProof = $script:foregroundArmProof",
        "$baselineProbe.foregroundHwnd -eq $armNativeSample.ForegroundHwnd.ToString()",
        "$baselineProbe.focusHwnd -eq $armNativeSample.FocusHwnd.ToString()",
        "$baselineProbe.cursor.x -eq $armNativeSample.CursorX",
        "$baselineProbe.inputDesktop -ceq [string]$armNativeSample.InputDesktop",
        "$baselineProbe.foregroundArmRequestedGeneration -eq $foregroundArmRequestGeneration",
        "$baselineProbe.foregroundArmAcknowledgedGeneration -eq $foregroundArmRequestGeneration",
        "$baselineProbe.foregroundArmRequestCount -eq 1",
        "$baselineProbe.foregroundArmAcknowledgementCount -eq 1",
        "$baselineProbe.foregroundArmLeftMouseDownCount -eq 1",
        "$baselineProbe.foregroundArmLeftMouseUpCount -eq 1",
        "$baselineProbe.foregroundArmButtonEnabled -eq $true",
        "$baselineProbe.statePublicationGeneration -gt [Int64]$foregroundArm.fixtureState.statePublicationGeneration",
        "Capture-InvariantProbe -AfterStatePublicationGeneration ([Int64]$foregroundArm.fixtureState.statePublicationGeneration)",
        "Capture-InvariantProbe -AfterStatePublicationGeneration $minimumPublicationGeneration",
        "stale valid publication",
        "$publicationSequence = @(11, 12, 13, 14, 14, 15, 15, 16)",
        "$armProbe.stateReads -ne 8",
        "$script:foregroundArmProof.baselineContinuityMatched = $true",
        "$baselineProbe.focusHwnd -eq [string]$script:fixtureReady.armButtonHwnd",
        "$script:runStage = \"rebind-post-arm-helper-readiness\"",
        "Wait-ForBoundHelperWorker $helperProcess $initialHelperSessionId $postArmHelperDescription",
        "[int]$postArmWorker.processId -eq $initialWorkerPid",
        "Complete-HelperTopologyRoundTrip $postArmHelperDescription $initialHelperSessionId $initialWorkerPid \"computer.status\" $statusResponse",
        "Save-StepResponse \"post-arm protocol-bound helper continuity\"",
        "$targetPid -eq $fixtureProcess.Id",
    ] {
        assert!(
            runner.contains(required),
            "Windows foreground-arm runner contract is missing: {required}"
        );
    }

    let desktop_runtime_gate = runner.find("if (-not $SelfTest -and (").unwrap();
    let live_job_creation = runner
        .find("$script:ownedJob = $script:ownedJobType::new()")
        .unwrap();
    assert!(
        desktop_runtime_gate < live_job_creation,
        "the Windows PowerShell 5.1 live-runtime gate must precede every live child process"
    );

    let selection = runner
        .find("$script:runStage = \"select-exact-fixture-window\"")
        .unwrap();
    let request = runner
        .find("$script:runStage = \"request-foreground-arm\"")
        .unwrap();
    let delivery = runner
        .find("$script:runStage = \"wait-foreground-arm-request-delivery\"")
        .unwrap();
    let delivery_proof = runner
        .find("Save-StepRecord \"foreground arm request delivery\"")
        .unwrap();
    let prompt = runner.find("Write-Host \"ACTION REQUIRED:").unwrap();
    let wait = runner
        .find("$script:runStage = \"wait-foreground-arm\"")
        .unwrap();
    let proof = runner
        .find("Save-StepRecord \"foreground arm proof\"")
        .unwrap();
    let helper_rebind = runner
        .find("$script:runStage = \"rebind-post-arm-helper-readiness\"")
        .unwrap();
    let continuity = runner
        .find("$script:foregroundArmProof.baselineContinuityMatched = $true")
        .unwrap();
    let baseline = runner
        .find("$script:runStage = \"baseline-status-and-observation\"")
        .unwrap();
    assert!(
        selection < request
            && request < delivery
            && delivery < delivery_proof
            && delivery_proof < prompt
            && prompt < wait
            && wait < continuity
            && continuity < proof
            && proof < helper_rebind
            && helper_rebind < baseline
    );
    let pre_action_arm_boundary = &runner[selection..baseline];
    for forbidden in [
        "Invoke-LbbCommand ",
        "Invoke-LbbCommandResponse ",
        "Start-LbbCommandRequest ",
    ] {
        assert!(
            !pre_action_arm_boundary.contains(forbidden),
            "effectful product command appeared before foreground arming completed: {forbidden}"
        );
    }

    for required in [
        "[Parameter(ParameterSetName = \"SelfTest\", Mandatory = $true)]",
        "public static void RunSelfTest()",
        "RecordForegroundArmRequest(0)",
        "RecordForegroundArmRequest(41)",
        "ForegroundArmRequestCount != 2",
        "TryAcknowledgeForegroundArm(40)",
        "TryAcknowledgeForegroundArm(41)",
        "RecordForegroundArmRequest(42)",
        "The foreground-arm generation state machine failed its self-test.",
        "The foreground-arm input-attempt counters failed their self-test.",
        "The foreground-arm button-enabled receipt failed its self-test.",
        "Windows computer-use fixture self-test passed.",
        "internal const int ForegroundArmMessage = 0x8126;",
        "armButton.Name = \"ForegroundArmButton\";",
        "armButton.Enabled = false;",
        "armButton.Text = \"CLICK TO ARM\";",
        "protected override bool ShowWithoutActivation",
        "internal const string StableWindowTitle = \"LBB Foreground Sentinel\";",
        "statusLabel.Text = \"ACTION REQUIRED\\r\\nClick once, then stop using this session\";",
        "Text = StableWindowTitle;",
        "statusLabel.Text = \"ARMED\\r\\nDo not use this session until the run finishes\";",
        "armButton.MouseDown +=",
        "armButton.MouseUp +=",
        "eventArgs.Button != MouseButtons.Left",
        "!armButton.ClientRectangle.Contains(eventArgs.Location)",
        "armButton.LostFocus += delegate { pressedArmGeneration = 0; };",
        "pressed != FixtureRuntime.ForegroundArmRequestedGeneration",
        "NativeMethods.GetForegroundWindow() != Handle",
        "NativeMethods.GetFocus() != armButton.Handle",
        "FixtureRuntime.TryAcknowledgeForegroundArm(pressed)",
        "FixtureRuntime.RecordForegroundArmLeftMouseDown()",
        "FixtureRuntime.RecordForegroundArmLeftMouseUp()",
        "FixtureRuntime.MarkForegroundArmButtonEnabled()",
        "protected override void OnDeactivate(EventArgs eventArgs)",
        "pressedArmGeneration = 0;",
        "Interlocked.CompareExchange(ref foregroundArmAcknowledgedGeneration, generation, 0)",
        "ready[\"armButtonHwnd\"]",
        "state[\"foregroundArmRequestedGeneration\"]",
        "state[\"foregroundArmAcknowledgedGeneration\"]",
        "state[\"foregroundArmLeftMouseDownCount\"]",
        "state[\"foregroundArmLeftMouseUpCount\"]",
        "state[\"foregroundArmButtonEnabled\"]",
        "state[\"statePublicationGeneration\"] = Interlocked.Increment(ref statePublicationGeneration);",
    ] {
        assert!(
            fixture.contains(required),
            "Windows foreground-arm fixture contract is missing: {required}"
        );
    }

    let request_state_machine = fixture
        .split("internal static bool RecordForegroundArmRequest(int generation)")
        .nth(1)
        .unwrap()
        .split("internal static bool TryAcknowledgeForegroundArm")
        .next()
        .unwrap();
    let count_attempt = request_state_machine
        .find("Interlocked.Increment(ref foregroundArmRequestCount);")
        .unwrap();
    let deduplicate = request_state_machine
        .find("if (previous == generation)")
        .unwrap();
    assert!(
        count_attempt < deduplicate,
        "every positive foreground-arm request attempt must be counted before duplicate rejection"
    );

    let sentinel = fixture
        .split("internal sealed class SentinelForm : Form")
        .nth(1)
        .unwrap()
        .split("internal sealed class OccluderForm : Form")
        .next()
        .unwrap();
    let mouse_down = sentinel
        .split("armButton.MouseDown +=")
        .nth(1)
        .unwrap()
        .split("armButton.LostFocus +=")
        .next()
        .unwrap();
    for required in [
        "eventArgs.Button != MouseButtons.Left",
        "!armButton.ClientRectangle.Contains(eventArgs.Location)",
        "NativeMethods.GetForegroundWindow() != Handle",
        "NativeMethods.GetFocus() != armButton.Handle",
    ] {
        assert!(
            mouse_down.contains(required),
            "Windows foreground-arm mouse-down is missing: {required}"
        );
    }
    assert!(!sentinel.contains("Activate();"));
    assert!(!sentinel.contains("Focus();"));
    assert!(!sentinel.contains("armButton.Click +="));
    assert!(!sentinel.contains("PerformClick"));
    assert!(sentinel.contains("protected override bool ShowWithoutActivation"));
    assert!(
        !sentinel.contains("WS_EX_NOACTIVATE"),
        "the sentinel must avoid activation only when first shown; the deliberate click must still activate and focus it"
    );
    let mouse_up = sentinel
        .split("armButton.MouseUp +=")
        .nth(1)
        .unwrap()
        .split("Controls.Add(armButton)")
        .next()
        .unwrap();
    assert!(
        !mouse_up.contains("armButton.Enabled = false"),
        "the acknowledged button must keep recording a duplicate click attempt so the exactly-once proof fails closed"
    );
    assert_eq!(
        sentinel.matches("TryAcknowledgeForegroundArm(").count(),
        1,
        "the fixture UI must acknowledge only from its mouse-release handler"
    );
    let production_runtime = fixture
        .split("public static void RunSelfTest()")
        .next()
        .unwrap();
    assert_eq!(
        production_runtime
            .matches("internal static bool TryAcknowledgeForegroundArm(")
            .count(),
        1,
        "the production fixture runtime must expose one acknowledgement transition"
    );
    let arm_sources = format!("{runner}\n{fixture}");
    for forbidden in [
        "SetForegroundWindow",
        "AttachThreadInput",
        "AllowSetForegroundWindow",
        "SendInput",
        "keybd_event",
        "mouse_event",
        "SwitchToThisWindow",
        "LockSetForegroundWindow",
    ] {
        assert!(
            !arm_sources.contains(forbidden),
            "foreground-arm acceptance must not use a focus or global-input forcing API: {forbidden}"
        );
    }

    for required in [
        "$armProbe.stateReads -ne 8",
        "$armProbe.nativeReads -ne 8",
        "stale acknowledgement",
        "missing click",
        "duplicate request",
        "duplicate acknowledgement",
        "duplicate left mouse down",
        "duplicate left mouse up",
        "wrong button identity",
        "native topology mismatch",
        "foreground mismatch",
        "focus mismatch",
        "cursor unavailable",
        "input desktop unavailable",
        "perpetual signature churn",
        "stale valid publication",
        "The foreground-arm request-delivery predicate failed its synthetic",
        "partial mouse down",
        "duplicate input edges",
        "button disabled",
        "wrong button",
        "The foreground-arm wait accepted a stale, unstable, or incomplete synthetic sequence.",
        "The foreground-arm wait did not fail closed for the synthetic",
    ] {
        assert!(
            runner.contains(required),
            "Windows foreground-arm self-test is missing: {required}"
        );
    }
}

#[test]
fn windows_foreground_arm_operator_markers_are_atomic_and_notification_only() {
    let runner = fs::read_to_string("scripts/test-windows-computer-use.ps1")
        .unwrap()
        .replace("\r\n", "\n");
    let development = fs::read_to_string("docs/DEVELOPMENT.md").unwrap();
    let release_verifier =
        fs::read_to_string("scripts/verify-release-acceptance-evidence.sh").unwrap();

    assert!(
        release_verifier.contains(".expectedVisibleWindowTitle == \"LBB Foreground Sentinel\"")
    );
    assert!(!release_verifier.contains("\"LBB Windows Acceptance - ACTION REQUIRED\""));

    for required in [
        "function Write-NewOperatorMarker {",
        "[ValidateSet(\"foreground-arm-request.json\", \"foreground-arm-received.json\")]",
        "[IO.FileMode]::CreateNew",
        "[IO.FileShare]::None",
        "$stream.Flush($true)",
        "[IO.File]::Move($temporaryPath, $finalPath)",
        "Operator markers are create-once for this runner and cannot be overwritten by it.",
        "function New-ForegroundArmRequestMarker {",
        "schemaVersion = 2",
        "productVersion = $ProductVersion",
        "status = if ($operatorActionRequired) { \"action-required\" } else { \"already-armed\" }",
        "preferredRelaySurface = \"windows-computer-use-app-share\"",
        "fallbackRelaySurface = \"human-on-windows-session\"",
        "expectedVisibleWindowTitle",
        "expectedVisibleButtonText",
        "expectedAccessibleName = \"Click to arm Windows acceptance\"",
        "action = if ($operatorActionRequired) { \"single-left-click\" } else { \"none\" }",
        "stopUiAfterAction = $true",
        "requiresSeparateAuthorization = $true",
        "markerGrantsAuthorization = $false",
        "markerGrantsConsent = $false",
        "externalOneShotConsentRequired = $true",
        "visualConfirmationRequired = $true",
        "maximumClickAttempts = if ($operatorActionRequired) { 1 } else { 0 }",
        "retryOnUnknownOutcome = $false",
        "inputStateAtPublication = $InputStateAtPublication",
        "function New-ForegroundArmReceivedMarker {",
        "An operator received marker requires the complete click and stable-native-sample proof.",
        "exactClickCountsMatched = $true",
        "notificationOnly = $true",
        "acceptedAsAuthority = $false",
        "rawWindowHandlesRecorded = $false",
        "rawCursorCoordinatesRecorded = $false",
        "pathsRecorded = $false",
        "secretsRecorded = $false",
        "The foreground-arm request marker failed its exact-schema self-test.",
        "The operator marker writer failed its atomic create-once self-test.",
        "The foreground-arm request marker did not suppress a duplicate-click prompt after an early valid acknowledgement.",
        "The foreground-arm received marker accepted an incomplete or duplicate-click proof.",
    ] {
        assert!(
            runner.contains(required),
            "Windows foreground-arm operator-marker contract is missing: {required}"
        );
    }

    let request_factory = runner
        .split("function New-ForegroundArmRequestMarker {")
        .nth(1)
        .unwrap()
        .split("function New-ForegroundArmReceivedMarker {")
        .next()
        .unwrap();
    for forbidden in [
        "pid =",
        "hwnd =",
        "cursorX =",
        "cursorY =",
        "token =",
        "generation =",
    ] {
        assert!(
            !request_factory.contains(forbidden),
            "operator request marker exposes an unnecessary raw identity or secret field: {forbidden}"
        );
    }

    let live_runner = runner
        .split("$sessionId = (Get-Process -Id $PID).SessionId")
        .nth(1)
        .unwrap();
    let delivery_proof = live_runner
        .find("Save-StepRecord \"foreground arm request delivery\"")
        .unwrap();
    let request_marker = live_runner
        .find("$foregroundArmRequestMarker = New-ForegroundArmRequestMarker")
        .unwrap();
    let prompt = live_runner.find("Write-Host \"ACTION REQUIRED:").unwrap();
    let stable_wait = live_runner
        .find("$foregroundArm = Wait-ForStableForegroundArm")
        .unwrap();
    let received_marker = live_runner
        .find("$foregroundArmReceivedMarker = New-ForegroundArmReceivedMarker")
        .unwrap();
    let baseline = live_runner
        .find("$baselineProbe = Capture-InvariantProbe")
        .unwrap();
    assert!(
        delivery_proof < request_marker
            && request_marker < prompt
            && prompt < stable_wait
            && stable_wait < received_marker
            && received_marker < baseline,
        "operator markers are not ordered around the existing click/native proof boundary"
    );
    assert_eq!(
        live_runner.matches("$foregroundArmRequestMarker").count(),
        2,
        "the live runner must only construct and write the request marker"
    );
    assert_eq!(
        live_runner.matches("$foregroundArmReceivedMarker").count(),
        2,
        "the live runner must only construct and write the received marker"
    );
    for forbidden in [
        "ReadAllText($foregroundArmRequestMarker",
        "ReadAllBytes($foregroundArmRequestMarker",
        "Get-Content $foregroundArmRequestMarker",
        "ReadAllText($foregroundArmReceivedMarker",
        "ReadAllBytes($foregroundArmReceivedMarker",
        "Get-Content $foregroundArmReceivedMarker",
    ] {
        assert!(
            !live_runner.contains(forbidden),
            "the live runner must never consume an operator marker as authority: {forbidden}"
        );
    }

    assert_eq!(runner.matches("Save-StepResponse \"").count(), 36);
    assert_eq!(runner.matches("Save-StepRecord \"").count(), 26);
    assert_eq!(runner.matches("Save-ObservationScreenshot $").count(), 18);
    assert_eq!(runner.matches("Save-SanitizedDesktopCrop \"").count(), 2);
    assert!(development.contains(
        "exactly 88 files: three fixture records, 62 step records, 20 sanitized screenshots, two operator notifications, and `summary.json`"
    ));
}

#[test]
fn windows_acceptance_coordinator_is_single_attempt_non_ui_and_fail_closed() {
    let coordinator = fs::read_to_string("scripts/run-windows-computer-use-acceptance.ps1")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "#requires -Version 5.1",
        "[ValidateSet(\"Start\", \"Follow\", \"SelfTest\")]",
        "$script:SuccessMessage = \"Windows computer-use acceptance coordinator self-test passed.\"",
        "$script:AttemptReservationSchemaVersion = 2",
        "function Start-Coordinator {",
        "function Follow-Coordinator {",
        "function Invoke-SelfTest {",
        "\"SelfTest\" { Invoke-SelfTest; return }",
        "\"Follow\" { Follow-Coordinator; return }",
        "\"Start\" { Start-Coordinator; return }",
        "status = \"terminal-attempt-boundary\"",
        "candidateExecutionState = \"unknown-after-this-record\"",
        "\"candidate-execution-unknown\"",
        "retryOnUnknownOutcome = $false",
        "retryAllowed = $false",
        "status = \"failed-closed\"",
        "uiActionAllowed = $false",
        "stopUiAfterAction = $true",
        "maximumClickAttempts = [int]$handoff.maximumClickAttempts",
        "runner.stdout.log",
        "runner.stderr.log",
        "watcher.stdout.log",
        "watcher.stderr.log",
        "worker.stdout.log",
        "worker.stderr.log",
        "[IO.FileMode]::CreateNew",
        "[IO.FileShare]::None",
        "[IO.File]::Move($temporaryPath, $Path)",
        "function Get-WhitelistedWorkerEnvironment {",
        "function Start-DetachedWorkerProcess {",
        "function Get-CandidateAttemptReservationPath {",
        "function Reserve-CandidateAttempt {",
        "function Get-AttemptReservationRelationship {",
        "function Copy-FileToPrivateStage {",
        "function New-WorkerLifetimeJob {",
        "JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE",
        "AssignProcessToJobObject(candidate, GetCurrentProcess())",
        "$runnerInfo.EnvironmentVariables[\"LBB_TOKEN\"] = $token",
        "$runnerInfo.EnvironmentVariables.Remove(\"LBB_TOKEN\")",
        "$token = $null",
        "-Suite\", \"All\", \"-ShowOccluder\"",
        "$watcherAccepted = $false",
        "$handoffPublished = $false",
        "$acceptanceCompleted = (",
        "$watcherAccepted -and",
        "$handoffPublished -and",
        "$summaryPassed -and",
        "$summary.passed -is [bool]",
        "-not [IO.File]::Exists($files.Failure)",
        "status = if ($acceptanceCompleted) { \"completed\" } else { \"failed-closed\" }",
        "if (-not $acceptanceCompleted) {",
        "\"watcher-handoff-missing\"",
        "\"runner-summary-failed\"",
        "consumerMustDeduplicateByRequestId = $true",
        "externalAuthorizationVerifiedByWatcher = $false",
        "Candidate input changes minted a second per-version attempt key.",
        "Follow accepted a post-boundary record without the persistent reservation.",
        "Follow persistent-boundary-only self-test failed.",
        "A refused duplicate changed the persistent attempt reservation bytes.",
        "A post-boundary validation failure did not publish terminal state.",
        "Follow confused another coordinator's reservation with its own.",
        "Follow changed a foreign coordinator's local not-started failure.",
        "Follow accepted local intent owned by a foreign reservation.",
        "Follow accepted a not-started failure after its owned reservation.",
        "Write-Output $script:SuccessMessage",
    ] {
        assert!(
            coordinator.contains(required),
            "Windows acceptance coordinator is missing: {required}"
        );
    }

    let terminal_reason = coordinator
        .split("Write-TerminalFailure $files \"runner-finished\"")
        .nth(1)
        .unwrap()
        .split("Write-CreateOnceJson $files.Final")
        .next()
        .unwrap();
    assert!(
        terminal_reason.find("\"runner-summary-failed\"").unwrap()
            < terminal_reason.find("\"watcher-handoff-missing\"").unwrap(),
        "a concrete failed runner summary must not be masked as a missing watcher handoff"
    );

    let lower = coordinator.to_ascii_lowercase();
    for forbidden in [
        "start-job",
        "invoke-expression",
        "cmd.exe",
        "tee-object",
        "-executionpolicy",
        "-execution-policy",
        "get-filehash",
        "setforegroundwindow",
        "attachthreadinput",
        "sendinput",
        "mouse_event",
        "setcursorpos",
        "postmessage",
        "sendmessage",
        "performclick",
        "system.windows.forms",
        "uiautomation",
        "retryonunknownoutcome = $true",
        "retryallowed = $true",
    ] {
        assert!(
            !lower.contains(forbidden),
            "Windows acceptance coordinator contains a forbidden retry, shell, module-discovery, or UI-control primitive: {forbidden}"
        );
    }

    let attempt_key = coordinator
        .split("function Get-CandidateAttemptKey {")
        .nth(1)
        .unwrap()
        .split("function Resolve-TrustedLocalAppData {")
        .next()
        .unwrap();
    let attempt_identity = attempt_key
        .split("$identity = ")
        .nth(1)
        .unwrap()
        .split("\n")
        .next()
        .unwrap();
    assert_eq!(
        attempt_identity, "@($script:AttemptLedgerDomain, $CandidateVersion) -join \"`n\"",
        "the irrevocable attempt key must remain version-only so rebuilt bytes cannot mint a retry"
    );

    let worker = coordinator
        .split("function Invoke-CoordinatorWorker {")
        .nth(1)
        .unwrap()
        .split("function Get-BootstrapFieldNames {")
        .next()
        .unwrap();
    assert_eq!(
        worker
            .matches("$runnerCapture = Start-CapturedProcess $runnerInfo $files.RunnerOut $files.RunnerErr")
            .count(),
        1,
        "the worker must launch exactly one bound acceptance runner"
    );
    assert_eq!(
        worker
            .matches("$watcherCapture = Start-CapturedProcess $watcherInfo $files.WatcherOut $files.WatcherErr")
            .count(),
        1,
        "the worker must launch exactly one read-only foreground watcher"
    );
    let launch_intent = worker.find("Write-CreateOnceJson $files.Intent").unwrap();
    let runner_launch = worker
        .find("$runnerCapture = Start-CapturedProcess $runnerInfo")
        .unwrap();
    let runner_record_publish = worker
        .find("Write-CreateOnceJson $files.Runner $runnerRecord")
        .unwrap();
    let runner_started = worker.find("Write-CreateOnceJson $files.Runner").unwrap();
    let watcher_launch = worker
        .find("$watcherCapture = Start-CapturedProcess $watcherInfo")
        .unwrap();
    assert!(
        launch_intent < runner_launch
            && runner_launch < runner_record_publish
            && runner_record_publish == runner_started
            && runner_started < watcher_launch,
        "the terminal candidate intent boundary, runner identity record, and watcher launch are misordered"
    );

    let runner_arguments = worker
        .split("$runnerArguments = @(")
        .nth(1)
        .unwrap()
        .split("$runnerInfo = New-ProcessStartInfo")
        .next()
        .unwrap();
    assert!(
        !runner_arguments.contains("LBB_TOKEN") && !runner_arguments.contains("$token"),
        "the bridge token must never enter candidate argv"
    );
    let config = coordinator
        .split("$config = [ordered]@{")
        .nth(1)
        .unwrap()
        .split("Write-CreateOnceJson $files.Config $config")
        .next()
        .unwrap();
    assert!(
        !config.contains("LBB_TOKEN") && !config.contains("token ="),
        "the bridge token must never enter the durable coordinator configuration"
    );

    for required in [
        "$runnerCapture = Start-CapturedProcess $runnerInfo $files.RunnerOut $files.RunnerErr",
        "$watcherCapture = Start-CapturedProcess $watcherInfo $files.WatcherOut $files.WatcherErr",
        "-StdoutPath $files.WorkerOut",
        "-StderrPath $files.WorkerErr",
        "PROC_THREAD_ATTRIBUTE_HANDLE_LIST",
        "stdoutPath, GENERIC_WRITE, FILE_SHARE_READ",
        "stderrPath, GENERIC_WRITE, FILE_SHARE_READ",
    ] {
        assert!(
            coordinator.contains(required),
            "the coordinator does not retain a distinct durable process stream: {required}"
        );
    }

    let atomic_writer = coordinator
        .split("function Write-CreateOnceJson {")
        .nth(1)
        .unwrap()
        .split("function Read-BoundedJson {")
        .next()
        .unwrap();
    let flush = atomic_writer.find("$stream.Flush($true)").unwrap();
    let publish = atomic_writer
        .find("[IO.File]::Move($temporaryPath, $Path)")
        .unwrap();
    assert!(
        flush < publish && !atomic_writer.contains("[IO.FileShare]::Read"),
        "coordinator records must be flushed to a private temporary file before atomic publication"
    );

    let request_marker = worker
        .find("[IO.File]::Exists($requestMarkerPath)")
        .unwrap();
    assert!(
        request_marker < watcher_launch,
        "the watcher budget must not start before the foreground-arm request marker exists"
    );
    assert!(
        coordinator.contains(
            "$process.StartTime.ToUniversalTime().Ticks -eq $StartedAtUtc.ToUniversalTime().Ticks"
        ),
        "coordinator process binding must compare exact UTC ticks"
    );
    assert!(
        coordinator.contains("uiActionAllowed = $false")
            && !coordinator.contains("uiActionAllowed = $actionRequired"),
        "the non-authoritative watcher notification must never become click permission"
    );
    let follow = coordinator
        .split("function Follow-Coordinator {")
        .nth(1)
        .unwrap()
        .split("function Remove-SelfTestStreamFiles {")
        .next()
        .unwrap();
    let terminal_projection = coordinator
        .split("function Get-TerminalFollowOutput {")
        .nth(1)
        .unwrap()
        .split("function Follow-Coordinator {")
        .next()
        .unwrap();
    assert!(
        terminal_projection
            .find("[IO.File]::Exists($Files.Failure)")
            .unwrap()
            < terminal_projection
                .find("[IO.File]::Exists($Files.Final)")
                .unwrap(),
        "Follow must preserve terminal failure stage/reason before considering Final"
    );
    assert!(
        follow.contains("bound-worker-not-alive")
            && follow.contains("Test-BoundProcessAlive ([int]$workerRecord.workerPid"),
        "Follow must fail closed when the exact retained worker is no longer alive"
    );
    assert!(
        coordinator.contains("Follow runner-finalizing self-test failed.")
            && follow.contains("-Phase \"runner-finalizing\"")
            && follow.contains("-Phase \"worker-guard-ownership-transfer\"")
            && follow.contains("-Phase \"runner-starting-or-waiting-for-handoff\""),
        "Follow must distinguish a live worker finalizing an exited runner from a dead worker"
    );
}

#[test]
fn windows_acceptance_coordinator_publication_and_detached_environment_are_closed() {
    let coordinator = fs::read_to_string("scripts/run-windows-computer-use-acceptance.ps1")
        .unwrap()
        .replace("\r\n", "\n");
    let atomic_writer = coordinator
        .split("function Write-CreateOnceJson {")
        .nth(1)
        .unwrap()
        .split("function Read-BoundedJson {")
        .next()
        .unwrap();
    for required in [
        "$directory = Resolve-OrdinaryPath ([IO.Path]::GetDirectoryName($Path)) $false \"Coordinator record parent\"",
        "$temporaryLeaf = \".\" + [IO.Path]::GetFileName($Path) + \".\" + [Guid]::NewGuid().ToString(\"N\") + \".tmp\"",
        "$temporaryPath = Assert-NewChildPath $directory $temporaryLeaf",
        "[IO.FileMode]::CreateNew",
        "[IO.FileAccess]::Write",
        "[IO.FileShare]::None",
        "$stream.Flush($true)",
        "[IO.File]::Move($temporaryPath, $Path)",
        "if ([IO.File]::Exists($temporaryPath)) {",
        "[IO.File]::Delete($temporaryPath)",
    ] {
        assert!(
            atomic_writer.contains(required),
            "atomic coordinator record publication is missing: {required}"
        );
    }
    let temporary_path = atomic_writer
        .find("$temporaryPath = Assert-NewChildPath")
        .unwrap();
    let open = atomic_writer.find("[IO.File]::Open(").unwrap();
    let flush = atomic_writer.find("$stream.Flush($true)").unwrap();
    let publish = atomic_writer
        .find("[IO.File]::Move($temporaryPath, $Path)")
        .unwrap();
    let cleanup = atomic_writer
        .find("[IO.File]::Delete($temporaryPath)")
        .unwrap();
    assert!(temporary_path < open && open < flush && flush < publish && publish < cleanup);
    for forbidden in [
        "[IO.File]::Open($Path",
        "[IO.File]::WriteAllText($Path",
        "[IO.File]::WriteAllBytes($Path",
        "[IO.FileMode]::Create,",
        "[IO.FileMode]::OpenOrCreate",
        "[IO.FileMode]::Append",
        "[IO.FileShare]::Read",
    ] {
        assert!(
            !atomic_writer.contains(forbidden),
            "atomic publication writes or exposes the final pathname before publication: {forbidden}"
        );
    }
    let terminal_writer = coordinator
        .split("function Write-TerminalFailure {")
        .nth(1)
        .unwrap()
        .split("function Get-WorkerConfiguration {")
        .next()
        .unwrap();
    assert!(terminal_writer.contains("catch [IO.IOException]"));
    assert!(terminal_writer.contains("if (-not [IO.File]::Exists($Files.Failure)) { throw }"));

    let allowlist = coordinator
        .split("function Get-WhitelistedWorkerEnvironment {")
        .nth(1)
        .unwrap()
        .split("function Start-DetachedWorkerProcess {")
        .next()
        .unwrap();
    for required in [
        "\"SystemRoot\"",
        "\"TEMP\"",
        "\"TMP\"",
        "\"USERPROFILE\"",
        "$value = [Environment]::GetEnvironmentVariable($name, \"Process\")",
        "$environment[$name] = $value",
        "if (-not $environment.Contains($required)",
    ] {
        assert!(
            allowlist.contains(required),
            "detached worker allowlist is missing: {required}"
        );
    }
    assert!(!allowlist.contains("GetEnvironmentVariables("));
    assert!(!allowlist.contains("\"PATH\""));

    let reservation_writer = coordinator
        .split("function Reserve-CandidateAttempt {")
        .nth(1)
        .unwrap()
        .split("function ConvertTo-NativeArgument {")
        .next()
        .unwrap();
    assert!(reservation_writer.contains(
        "schemaVersion = $script:AttemptReservationSchemaVersion\n            kind = \"windows-acceptance-attempt-reservation\"\n            status = \"reserved-no-retry\"\n            productVersion = $CandidateVersion\n            attemptKey = $AttemptKey\n            checksumManifestSha256 = $ManifestSha256.ToLowerInvariant()\n            coordinatorInstanceId = $CoordinatorInstanceId\n            retryAllowed = $false"
    ));

    let detached_launcher = coordinator
        .split("function Start-DetachedWorkerProcess {")
        .nth(1)
        .unwrap()
        .split("function Stop-DetachedWorkerProcessExact {")
        .next()
        .unwrap();
    for required in [
        "NativeDetachedWorkerLauncher",
        "CreateProcess(",
        "BuildEnvironment(environment)",
        "SortedDictionary<string, string>",
        "PROC_THREAD_ATTRIBUTE_HANDLE_LIST",
        "PROC_THREAD_ATTRIBUTE_JOB_LIST",
        "JOB_OBJECT_LIMIT_BREAKAWAY_OK = 0x00000800",
        "JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000",
        "limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE |\n        JOB_OBJECT_LIMIT_BREAKAWAY_OK",
        "CREATE_SUSPENDED | CREATE_UNICODE_ENVIRONMENT | CREATE_BREAKAWAY_FROM_JOB",
        "IsProcessInJob(nativeProcess.Process, guardJob, out guarded)",
        "ResumeThread(nativeProcess.Thread)",
        "TransferGuardOwnership()",
        "return [LbbCoordinator.NativeDetachedWorkerLauncher]::Start(",
    ] {
        assert!(
            detached_launcher.contains(required),
            "detached worker environment boundary is missing: {required}"
        );
    }
    let environment = detached_launcher
        .find("BuildEnvironment(environment)")
        .unwrap();
    let guard = detached_launcher
        .find("guardJob = CreateKillOnCloseJob()")
        .unwrap();
    let create = detached_launcher
        .find("bool started = CreateProcess(")
        .unwrap();
    let verify = detached_launcher
        .find("IsProcessInJob(nativeProcess.Process, guardJob, out guarded)")
        .unwrap();
    let resume = detached_launcher
        .find("ResumeThread(nativeProcess.Thread)")
        .unwrap();
    let transfer = detached_launcher
        .find("ownershipTransferred = true")
        .unwrap();
    assert!(
        environment < guard
            && guard < create
            && create < verify
            && verify < resume
            && resume < transfer
    );
    assert!(!coordinator.contains("Start-Process"));

    let start = coordinator
        .split("function Start-Coordinator {")
        .nth(1)
        .unwrap()
        .split("function Write-FollowFailureOutput {")
        .next()
        .unwrap();
    let config_publish = start
        .find("Write-CreateOnceJson $files.Config $config")
        .unwrap();
    let start_publish = start.find("Write-CreateOnceJson $files.Start").unwrap();
    let pre_reservation_validation = start
        .find("Assert-ExactConfiguration $validatedConfig -BeforeReservation")
        .unwrap();
    let worker_launch = start.find("$worker = Start-DetachedWorkerProcess").unwrap();
    let coordinator_id = start
        .find("$coordinatorInstanceId = [Guid]::NewGuid().ToString(\"N\")")
        .unwrap();
    let config_id = start
        .find("coordinatorInstanceId = $coordinatorInstanceId")
        .unwrap();
    let start_id = start[start_publish..]
        .find("coordinatorInstanceId = $coordinatorInstanceId")
        .map(|position| start_publish + position)
        .unwrap();
    assert!(
        coordinator_id < config_id
            && config_id < config_publish
            && config_publish < start_publish
            && start_publish < start_id
            && start_publish < pre_reservation_validation
            && pre_reservation_validation < worker_launch,
        "Start must bind one opaque coordinator ID through config and Start before launching the retained worker"
    );
    assert_eq!(
        start
            .matches("coordinatorInstanceId = $coordinatorInstanceId")
            .count(),
        2,
        "Start must publish the opaque coordinator ID only in config and the Start record"
    );
    assert!(
        !start.contains("Reserve-CandidateAttempt"),
        "Start must not consume the persistent one-shot boundary before worker ownership"
    );
    for required in [
        "$workerEnvironment = Get-WhitelistedWorkerEnvironment",
        "$workerEnvironment[\"LBB_WINDOWS_COORDINATOR_WORKER_NONCE\"] = $workerNonce",
        "$workerEnvironment[\"LBB_WINDOWS_COORDINATOR_CONFIG\"] = $files.Config",
        "$worker = Start-DetachedWorkerProcess",
        "-Environment $workerEnvironment",
        "The detached worker allowlist retained an unlisted environment value.",
    ] {
        assert!(
            coordinator.contains(required),
            "production or self-test detached-environment proof is missing: {required}"
        );
    }
    assert_eq!(
        start.matches("$workerEnvironment[").count(),
        2,
        "Start may add only the worker nonce and private config path to the detached allowlist"
    );
    let handoff = start.find("$workerHandedOff = $true").unwrap();
    let guard_transfer = start.find("$worker.TransferGuardOwnership()").unwrap();
    let ownership_record = start
        .find("Write-CreateOnceJson $files.Ownership $ownershipRecord")
        .unwrap();
    let catch = start.find("\n    catch {").unwrap();
    let final_output = start.rfind("Write-Output $startOutput").unwrap();
    assert!(
        guard_transfer < ownership_record
            && ownership_record < handoff
            && handoff < catch
            && catch < final_output,
        "Start must transfer worker ownership before emitting to a fallible output consumer"
    );
    assert!(
        start.contains("if (-not $workerHandedOff -and $null -ne $worker)")
            && start
                .contains("if (-not $workerHandedOff -and -not [IO.File]::Exists($files.Failure))"),
        "Start may terminate or record failure only before durable worker handoff"
    );
    let launch_try = &start[..catch];
    assert!(
        !launch_try.contains("Write-Output"),
        "a closed Start output pipeline must not re-enter launch cleanup and kill the worker"
    );
    let worker = coordinator
        .split("function Invoke-CoordinatorWorker {")
        .nth(1)
        .unwrap()
        .split("function Get-BootstrapFieldNames {")
        .next()
        .unwrap();
    let ownership = worker.find("Assert-ExactOwnershipRecord `").unwrap();
    let prepared_process = worker.find("$runnerInfo = New-ProcessStartInfo `").unwrap();
    let fresh_pre_reservation_validation = worker[prepared_process..]
        .find("Assert-ExactConfiguration $config -BeforeReservation")
        .map(|position| prepared_process + position)
        .unwrap();
    let reservation = worker
        .find("$reservedAttemptPath = Reserve-CandidateAttempt")
        .unwrap();
    let reservation_call = worker[reservation..]
        .split("if ($reservedAttemptPath")
        .next()
        .unwrap();
    assert!(
        reservation_call.contains("-CoordinatorInstanceId ([string]$config.coordinatorInstanceId)")
    );
    let post_reservation_validation = worker[reservation..]
        .find("Assert-ExactConfiguration $config")
        .map(|position| reservation + position)
        .unwrap();
    let intent = worker.find("Write-CreateOnceJson $files.Intent").unwrap();
    let runner_launch = worker
        .find("$runnerCapture = Start-CapturedProcess $runnerInfo")
        .unwrap();
    let runner_record = worker
        .find("Write-CreateOnceJson $files.Runner $runnerRecord")
        .unwrap();
    assert!(
        ownership < prepared_process
            && prepared_process < fresh_pre_reservation_validation
            && fresh_pre_reservation_validation < reservation
            && reservation < post_reservation_validation
            && post_reservation_validation < intent
            && intent < runner_launch
            && runner_launch < runner_record,
        "worker ownership and runner preparation must precede the durable one-shot boundary, which must precede the sole runner launch and identity record"
    );
    assert_eq!(
        worker.matches("Reserve-CandidateAttempt").count(),
        1,
        "the production worker must publish exactly one persistent attempt boundary"
    );
    assert!(
        coordinator.contains("$relationship = Get-AttemptReservationRelationship $Config")
            && coordinator
                .contains("if ($relationship -ceq \"owned\" -or $relationship -ceq \"invalid\")")
            && coordinator.contains(
                "A post-boundary coordinator record has no persistent attempt reservation."
            )
            && coordinator
                .contains("Follow confused another coordinator's reservation with its own."),
        "ledger presence must classify an interrupted pre-runner boundary as outcome unknown"
    );
    for forbidden in [
        "[IO.File]::Delete($AttemptLedgerPath)",
        "[IO.File]::Delete([string]$Config.attemptLedgerPath)",
        "Remove-Item $AttemptLedgerPath",
    ] {
        assert!(
            !coordinator.contains(forbidden),
            "production code must never remove a persistent attempt boundary: {forbidden}"
        );
    }
    assert!(
        start[catch..final_output].contains("Get-ObservedAttemptState $files $config"),
        "Start cleanup must conservatively classify the authoritative ledger boundary"
    );
}

#[test]
fn windows_acceptance_coordinator_owns_children_and_delays_watcher_until_marker() {
    let coordinator = fs::read_to_string("scripts/run-windows-computer-use-acceptance.ps1")
        .unwrap()
        .replace("\r\n", "\n");
    let lifetime_job = coordinator
        .split("function Get-WorkerLifetimeSupportSource {")
        .nth(1)
        .unwrap()
        .split("function New-WorkerLifetimeSupportAssembly {")
        .next()
        .unwrap();
    for required in [
        "JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE = 0x00002000",
        "limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE",
        "OpenJobObject(",
        "JOB_OBJECT_QUERY | JOB_OBJECT_TERMINATE",
        "TerminateJobObject(job, 1)",
        "QueryInformationJobObject(",
        "accounting.ActiveProcesses == 0",
        "WaitForJobNameToDisappear(",
        "OpenJobObject(JOB_OBJECT_QUERY, false, name)",
        "ERROR_ALREADY_EXISTS = 183",
        "SetLastError(0)",
        "IntPtr candidate = CreateJobObject(IntPtr.Zero, name)",
        "int createError = Marshal.GetLastWin32Error()",
        "if (candidate == IntPtr.Zero)",
        "if (createError == 0)",
        "CreateJobObject returned an existing uninspected coordinator lifetime Job",
        "CloseJobHandleOnce(",
        "EnsureRecoveryTimeRemaining(",
        "recoveryDeadline.Elapsed.TotalMilliseconds >= timeoutMilliseconds",
        "SleepForRecoveryPoll(",
        "SetInformationJobObject(candidate, JobObjectExtendedLimitInformation, buffer, (uint)size)",
        "AssignProcessToJobObject(candidate, GetCurrentProcess())",
        "handle = candidate",
        "candidate = IntPtr.Zero",
    ] {
        assert!(
            lifetime_job.contains(required),
            "worker lifetime ownership is missing: {required}"
        );
    }
    assert!(lifetime_job.contains("allowChildBreakaway ? JOB_OBJECT_LIMIT_BREAKAWAY_OK : 0"));
    assert!(!lifetime_job.contains("JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK"));
    let recovery = lifetime_job
        .split("private WorkerLifetimeJob(")
        .nth(1)
        .unwrap()
        .split("private static IntPtr CreateFreshJobOnce(")
        .next()
        .unwrap();
    let deadline_start = recovery
        .find("Stopwatch recoveryDeadline = recoverExisting")
        .unwrap();
    let inspect_prior = recovery.find("IntPtr previous = OpenJobObject(").unwrap();
    let terminate_prior = recovery
        .find("TerminateAndWaitForEmpty(previous, recoveryDeadline")
        .unwrap();
    let close_inspected = recovery
        .find("CloseJobHandleOnce(\n              previous")
        .unwrap();
    let wait_for_absence = recovery
        .find("WaitForJobNameToDisappear(name, recoveryDeadline")
        .unwrap();
    let create_once = recovery.find("CreateFreshJobOnce(name)").unwrap();
    let check_after_create = recovery
        .find("The coordinator lifetime Job recovery deadline expired during fresh creation")
        .unwrap();
    let stop_deadline = recovery.find("recoveryDeadline.Stop()").unwrap();
    let configure = recovery.find("SetInformationJobObject(candidate").unwrap();
    let check_after_configure = recovery
        .find("The coordinator lifetime Job recovery deadline expired during configuration")
        .unwrap();
    let bind = recovery.find("AssignProcessToJobObject(candidate").unwrap();
    let check_after_bind = recovery
        .find("The coordinator lifetime Job recovery deadline expired during worker binding")
        .unwrap();
    let retain = recovery.find("handle = candidate").unwrap();
    assert!(
        deadline_start < inspect_prior
            && inspect_prior < terminate_prior
            && terminate_prior < close_inspected
            && close_inspected < wait_for_absence
            && wait_for_absence < create_once
            && create_once < check_after_create
            && check_after_create < configure
            && configure < check_after_configure
            && check_after_configure < bind
            && bind < check_after_bind
            && check_after_bind < stop_deadline
            && stop_deadline < retain,
        "one recovery deadline must cover inspection, exact teardown, namespace absence, single fresh creation, configuration, and binding"
    );

    let namespace_poll = lifetime_job
        .split("private static void WaitForJobNameToDisappear(")
        .nth(1)
        .unwrap()
        .split("private static void TerminateAndWaitForEmpty(")
        .next()
        .unwrap();
    let poll_open = namespace_poll
        .find("OpenJobObject(JOB_OBJECT_QUERY, false, name)")
        .unwrap();
    let poll_close = namespace_poll.find("CloseJobHandleOnce(").unwrap();
    let poll_sleep = namespace_poll.find("SleepForRecoveryPoll(").unwrap();
    assert!(poll_open < poll_close && poll_close < poll_sleep);
    assert!(namespace_poll.contains("openError == ERROR_FILE_NOT_FOUND"));

    let fresh_job = lifetime_job
        .split("private static IntPtr CreateFreshJobOnce(")
        .nth(1)
        .unwrap()
        .split("private static void WaitForJobNameToDisappear(")
        .next()
        .unwrap();
    assert_eq!(
        fresh_job
            .matches("CreateJobObject(IntPtr.Zero, name)")
            .count(),
        1
    );
    assert!(!fresh_job.contains("while (") && !fresh_job.contains("SleepForRecoveryPoll"));
    let create_candidate = fresh_job
        .find("IntPtr candidate = CreateJobObject(IntPtr.Zero, name)")
        .unwrap();
    let capture_status = fresh_job
        .find("int createError = Marshal.GetLastWin32Error()")
        .unwrap();
    let accept_fresh = fresh_job.find("if (createError == 0)").unwrap();
    let close_existing = fresh_job.find("CloseJobHandleOnce(").unwrap();
    let reject_existing = fresh_job.rfind("throw new Win32Exception(").unwrap();
    assert!(
        create_candidate < capture_status
            && capture_status < accept_fresh
            && accept_fresh < close_existing
            && close_existing < reject_existing,
        "fresh named-Job recovery must immediately capture status, accept only a fresh create, and close then reject every existing or nonzero-status handle"
    );
    assert!(!lifetime_job.contains("private static IntPtr CreateFreshJob("));
    assert!(coordinator.contains(
        "WorkerLifetimeJobName = \"Local\\LBBWindowsAcceptanceCoordinatorLifetimeJob-v1\""
    ));
    let self_test = coordinator
        .split("function Invoke-SelfTest {")
        .nth(1)
        .unwrap()
        .split("if ([Environment]::OSVersion.Platform")
        .next()
        .unwrap();
    for required in [
        "LifetimeJobCleanSelfTest-",
        "The clean-start self-test did not bind a fresh named Job.",
        "The exact prior named-Job owner tree was not live before recovery.",
        "LifetimeDelayedHandleSelfTest-",
        "NamedJobHandleLease]::WaitForEmpty($handle, 30000)",
        "$delayedHolderStatePath + \".empty\"",
        "$delayedBoundHolder.WaitForExit(5000)",
        "LifetimeTimeoutSelfTest-",
        "$timeoutHookState = [pscustomobject]@{ Invoked = $false }",
        "$timeoutHookState.Invoked",
        "\"System.TimeoutException\"",
        "$timeoutHolderStatePath + \".empty\"",
        "WaitForNameAbsenceForSelfTest(\n                $timeoutJobName",
        "LifetimeCreateRaceSelfTest-",
        "$raceHookState.Count -ne 1",
        "Test-Win32ErrorInChain $_.Exception 183",
        "The refused create race adopted or terminated the exact raced Job owner.",
        "WaitForNameAbsenceForSelfTest(\n                $raceJobName",
        "$boundWorkerProcess.WaitForExit(250)",
        "$boundSleeperProcess.WaitForExit(10000)",
        "$boundControlProcess.HasExited",
        "function Complete-SelfTestCapturedProcess",
        "function Remove-SelfTestStreamFiles",
        "^lbb-coordinator-self-test-[0-9a-f]{32}$",
        "if ($selfTestSucceeded) { Write-Output $script:SuccessMessage }",
    ] {
        assert!(
            coordinator.contains(required),
            "Windows native coordinator scenario proof is missing: {required}"
        );
    }
    for forbidden in [
        "$cleanJobName -ceq $script:WorkerLifetimeJobName",
        "$delayedJobName -ceq $script:WorkerLifetimeJobName",
        "$timeoutJobName -ceq $script:WorkerLifetimeJobName",
        "$raceJobName -ceq $script:WorkerLifetimeJobName",
    ] {
        assert!(self_test.contains(forbidden));
    }
    let mark_success = self_test.find("$selfTestSucceeded = $true").unwrap();
    let delete_root = self_test
        .find("[IO.Directory]::Delete($testRoot, $true)")
        .unwrap();
    let emit_success = self_test
        .find("if ($selfTestSucceeded) { Write-Output $script:SuccessMessage }")
        .unwrap();
    assert!(mark_success < delete_root && delete_root < emit_success);
    assert!(coordinator.contains("CREATE_BREAKAWAY_FROM_JOB"));
    assert!(coordinator.contains("PROC_THREAD_ATTRIBUTE_JOB_LIST"));
    assert!(coordinator.contains("$worker.TransferGuardOwnership()"));
    let worker = coordinator
        .split("function Invoke-CoordinatorWorker {")
        .nth(1)
        .unwrap()
        .split("function Get-BootstrapFieldNames {")
        .next()
        .unwrap();
    let mutex_acquire = worker.find("$exclusiveMutex.WaitOne(0)").unwrap();
    let lifetime_recovery = worker
        .find("$workerLifetimeJob = New-WorkerLifetimeJob")
        .unwrap();
    let worker_record = worker.find("Write-CreateOnceJson $files.Worker").unwrap();
    assert!(
        mutex_acquire < lifetime_recovery && lifetime_recovery < worker_record,
        "the stable mutex must serialize old-Job recovery before a new worker is admitted"
    );
    for required in [
        "-Name $script:WorkerLifetimeJobName",
        "-RecoverExisting",
        "-RecoveryTimeoutMilliseconds $script:WorkerLifetimeRecoveryMilliseconds",
    ] {
        assert!(
            worker.contains(required),
            "production worker lifetime recovery is missing: {required}"
        );
    }
    let support_loader = coordinator
        .split("function Initialize-WorkerLifetimeSupport {")
        .nth(1)
        .unwrap()
        .split("function New-WorkerLifetimeJob {")
        .next()
        .unwrap();
    for required in [
        "$assemblyBytes = [IO.File]::ReadAllBytes($resolved)",
        "$loadedAssemblySha256 = ([BitConverter]::ToString(\n            $sha256.ComputeHash($assemblyBytes)\n        )).Replace(\"-\", \"\").ToLowerInvariant()",
        "$null = [Reflection.Assembly]::Load($assemblyBytes)",
    ] {
        assert!(
            support_loader.contains(required),
            "worker support must hash and load the same exact byte array: {required}"
        );
    }
    assert!(!support_loader.contains("Add-Type -Path"));
    assert!(
        !coordinator.contains("ConvertTo-LowerHex"),
        "the production worker-support loader must not depend on an undefined hex helper"
    );

    let loader_self_test = coordinator
        .split("function Invoke-WorkerSupportLoaderSelfTest {")
        .nth(1)
        .unwrap()
        .split("function New-WorkerLifetimeJob {")
        .next()
        .unwrap();
    for required in [
        "LBB_COORDINATOR_WORKER_SUPPORT_SELF_TEST_NONCE",
        "$environmentNonce -cne $Nonce",
        "if (\"LbbCoordinator.WorkerLifetimeJob\" -as [type])",
        "-AssemblySha256 $incorrectSha256",
        "The worker lifetime support assembly hash does not match its private configuration.",
        "Initialize-WorkerLifetimeSupport `\n        -AssemblyPath $AssemblyPath `\n        -AssemblySha256 $AssemblySha256",
        "Local\\LBBWindowsAcceptanceCoordinatorLoaderSelfTest-$Nonce",
        "if (-not $probeJob.IsBound -or $probeJob.RecoveredExistingJob)",
        "[GC]::KeepAlive($probeJob)",
        "Worker lifetime support staged-loader self-test passed.",
    ] {
        assert!(
            loader_self_test.contains(required),
            "fresh staged worker-support loader self-test is missing: {required}"
        );
    }
    let nested_job_runner_self_test = coordinator
        .split("function Invoke-NestedJobRunnerSelfTest {")
        .nth(1)
        .unwrap()
        .split("function Invoke-SelfTest {")
        .next()
        .unwrap();
    for required in [
        "LBB_WINDOWS_NESTED_JOB_RUNNER_SELF_TEST_NONCE",
        "$lifetimeJob = New-WorkerLifetimeJob `\n            -AllowChildBreakaway `",
        "test-windows-computer-use.ps1",
        "$runnerCapture = Start-CapturedProcess $runnerInfo $runnerOut $runnerErr",
        "$runnerExit = Complete-CapturedProcess",
        "Windows computer-use acceptance self-test passed.",
        "Nested guard/lifetime Job runner self-test passed.",
    ] {
        assert!(
            nested_job_runner_self_test.contains(required),
            "guard-plus-lifetime runner self-test is missing: {required}"
        );
    }
    let self_test = coordinator
        .split("function Invoke-SelfTest {")
        .nth(1)
        .unwrap()
        .split("if ([Environment]::OSVersion.Platform")
        .next()
        .unwrap();
    for required in [
        "$workerSupportProbeScript = Copy-FileToPrivateStage",
        "(Get-FileSha256 $workerSupportProbeScript) -cne",
        "$workerSupportProbe = New-WorkerLifetimeSupportAssembly $workerSupportProbePath",
        "\"-File\", $workerSupportProbeScript",
        "\"-InternalWorkerSupportSelfTestPath\", $workerSupportProbe.Path",
        "\"-InternalWorkerSupportSelfTestSha256\", $workerSupportProbe.Sha256",
        "\"-InternalWorkerSupportSelfTestNonce\", $workerSupportProbeNonce",
        "Set-ExactProcessEnvironment `\n            $workerSupportProbeInfo `\n            (Get-WhitelistedWorkerEnvironment)",
        "$workerSupportProbeInfo.EnvironmentVariables[\n            \"LBB_COORDINATOR_WORKER_SUPPORT_SELF_TEST_NONCE\"\n        ] = $workerSupportProbeNonce",
        "$workerSupportProbeExit = Complete-CapturedProcess",
        "$workerSupportProbeFiles.Intent",
        "$workerSupportProbeFiles.Runner",
        "The staged loader self-test crossed a candidate execution boundary.",
        "The fresh staged worker-support loader self-test failed.",
        "WaitForNameAbsenceForSelfTest(\n            \"Local\\LBBWindowsAcceptanceCoordinatorLoaderSelfTest-$workerSupportProbeNonce\"",
        "$nestedRunnerSelfTestNonce = [Guid]::NewGuid().ToString(\"N\")",
        "$nestedRunnerSelfTestResult = @(Start-DetachedWorkerProcess",
        "\"-InternalNestedJobRunnerSelfTestNonce\", $nestedRunnerSelfTestNonce",
        "$nestedRunnerSelfTestProcess.TransferGuardOwnership()",
        "\"Nested guard/lifetime Job runner self-test passed.\"",
        "The exact guard-plus-lifetime Job runner self-test failed.",
        "The nested-Job runner self-test crossed a candidate execution boundary.",
    ] {
        assert!(
            self_test.contains(required),
            "coordinator self-test does not launch the fresh staged-loader probe: {required}"
        );
    }
    let staged_probe = self_test
        .find("$workerSupportProbe = New-WorkerLifetimeSupportAssembly")
        .unwrap();
    let in_memory_shortcut = self_test
        .find("$selfTestLifetimeJob = New-WorkerLifetimeJob -AllowChildBreakaway")
        .unwrap();
    assert!(
        staged_probe < in_memory_shortcut,
        "the fresh staged-loader probe must run before the in-memory self-test type is compiled"
    );

    let worker = coordinator
        .split("function Invoke-CoordinatorWorker {")
        .nth(1)
        .unwrap()
        .split("function Get-BootstrapFieldNames {")
        .next()
        .unwrap();
    let bind_job = worker
        .find("$workerLifetimeJob = New-WorkerLifetimeJob")
        .unwrap();
    assert!(worker[bind_job..].starts_with(
        "$workerLifetimeJob = New-WorkerLifetimeJob `\n            -AllowChildBreakaway `"
    ));
    let acquire_mutex = worker.find("$exclusiveMutex.WaitOne(0)").unwrap();
    let worker_record = worker.find("Write-CreateOnceJson $files.Worker").unwrap();
    let ownership_record = worker.find("Assert-ExactOwnershipRecord").unwrap();
    let attempt_boundary = worker
        .find("$reservedAttemptPath = Reserve-CandidateAttempt")
        .unwrap();
    let launch_intent = worker.find("Write-CreateOnceJson $files.Intent").unwrap();
    let runner_launch = worker
        .find("$runnerCapture = Start-CapturedProcess $runnerInfo")
        .unwrap();
    let runner_record_publish = worker
        .find("Write-CreateOnceJson $files.Runner $runnerRecord")
        .unwrap();
    assert!(
        acquire_mutex < bind_job
            && bind_job < worker_record
            && worker_record < ownership_record
            && ownership_record < attempt_boundary
            && attempt_boundary < launch_intent
            && launch_intent < runner_launch
            && runner_launch < runner_record_publish,
        "exclusive admission and old-Job recovery must complete before any candidate-attempt record or child"
    );
    assert!(
        !worker.contains("$runnerStarted"),
        "terminal attempt classification must come only from published state records"
    );
    let worker_catch = worker.rsplit("\n    catch {").next().unwrap();
    assert!(worker_catch.contains(
        "$attemptState = Get-ObservedAttemptState `\n            $files `\n            $config"
    ));
    for required in [
        "Local\\LBBWindowsAcceptanceCoordinator",
        "Write-TerminalFailure $files \"worker-exclusivity\" \"not-started\" \"another-coordinator-is-active\"",
        "Write-TerminalFailure $files \"worker-ownership\" \"not-started\" \"worker-ownership-unavailable\"",
        "$script:ProcessLifetimeCoordinatorMutex = $exclusiveMutex",
        "[GC]::KeepAlive($script:ProcessLifetimeCoordinatorMutex)",
        "[GC]::KeepAlive($workerLifetimeJob)",
    ] {
        assert!(
            worker.contains(required),
            "worker ownership fail-closed state is missing: {required}"
        );
    }
    assert!(
        !worker.contains("LBBWindowsAcceptanceCoordinator-v"),
        "the session exclusivity mutex must remain stable across product versions"
    );
    assert!(
        !worker.contains("$exclusiveMutex.ReleaseMutex()"),
        "the worker must retain its exclusion mutex until process teardown"
    );
    let runner_cleanup = worker.rfind("if ($null -ne $runnerCapture)").unwrap();
    let retain_mutex = worker
        .rfind("[GC]::KeepAlive($script:ProcessLifetimeCoordinatorMutex)")
        .unwrap();
    let keep_job = worker.rfind("[GC]::KeepAlive($workerLifetimeJob)").unwrap();
    assert!(runner_cleanup < retain_mutex && retain_mutex < keep_job);

    let capture = coordinator
        .split("function Start-CapturedProcess {")
        .nth(1)
        .unwrap()
        .split("function Complete-CapturedProcess {")
        .next()
        .unwrap();
    let capture_try = capture.find("    try {").unwrap();
    let stdout_open = capture.find("$stdout = [IO.File]::Open").unwrap();
    let stderr_open = capture.find("$stderr = [IO.File]::Open").unwrap();
    let process_create = capture
        .find("$process = [Diagnostics.Process]::new()")
        .unwrap();
    assert!(capture_try < stdout_open && stdout_open < stderr_open && stderr_open < process_create);
    for required in [
        "if ($null -ne $process) { $process.Dispose() }",
        "if ($null -ne $stdout) { $stdout.Dispose() }",
        "if ($null -ne $stderr) { $stderr.Dispose() }",
        "Captured-stream partial-acquisition self-test did not fail closed.",
        "Captured-stream partial-acquisition self-test left a locked path.",
    ] {
        assert!(
            coordinator.contains(required),
            "captured-process partial acquisition is not safely cleaned up or self-tested: {required}"
        );
    }

    let open_worker = coordinator
        .find("$boundWorkerProcess = [LbbCoordinator.DetachedWorkerProcess]::OpenExact")
        .unwrap();
    let open_sleeper = coordinator
        .find("$boundSleeperProcess = [LbbCoordinator.DetachedWorkerProcess]::OpenExact")
        .unwrap();
    let probe_transfer = coordinator
        .rfind("$ownershipLauncherProcess.TransferGuardOwnership()")
        .unwrap();
    let probe_assertion = coordinator
        .find("The launcher-exit durability probe did not retain only its intended processes.")
        .unwrap();
    assert!(
        open_worker < probe_transfer
            && open_sleeper < probe_transfer
            && probe_transfer < probe_assertion,
        "the ownership self-test must retain exact cleanup handles before its fallible transfer assertions"
    );

    let marker_directory = worker
        .find("$operatorDirectory = [IO.Path]::Combine($resolvedEvidenceDirectory, \"operator\")")
        .unwrap();
    let marker_path = worker
        .find("$requestMarkerPath = [IO.Path]::Combine($resolvedOperatorDirectory, \"foreground-arm-request.json\")")
        .unwrap();
    let marker_resolved = worker
        .find("Resolve-OrdinaryPath $requestMarkerPath $true \"Foreground-arm request marker\"")
        .unwrap();
    let marker_seen = worker.find("$requestMarkerAppeared = $true").unwrap();
    let watcher_gate = worker
        .find("if ($requestMarkerAppeared -and -not $runnerCapture.Process.HasExited)")
        .unwrap();
    let watcher_launch = worker
        .find("$watcherCapture = Start-CapturedProcess $watcherInfo")
        .unwrap();
    assert!(
        marker_directory < marker_path
            && marker_path < marker_resolved
            && marker_resolved < marker_seen
            && marker_seen < watcher_gate
            && watcher_gate < watcher_launch,
        "the watcher timeout must begin only after an ordinary request marker is published"
    );

    let liveness = coordinator
        .split("function Test-BoundProcessAlive {")
        .nth(1)
        .unwrap()
        .split("function Assert-InteractiveInputDesktop {")
        .next()
        .unwrap();
    assert!(liveness.contains("-not $process.HasExited"));
    assert!(liveness.contains(
        "$process.StartTime.ToUniversalTime().Ticks -eq $StartedAtUtc.ToUniversalTime().Ticks"
    ));
    for forbidden in [
        "[Math]::Abs",
        "TotalMilliseconds",
        "AddMilliseconds",
        "AddSeconds",
    ] {
        assert!(
            !liveness.contains(forbidden),
            "process identity must not use a liveness tolerance: {forbidden}"
        );
    }
    for required in [
        "Test-BoundProcessAlive $worker.Id $workerStartedAt",
        "Test-BoundProcessAlive ([int]$workerRecord.workerPid) $workerStartedAt",
        "Test-BoundProcessAlive ([int]$runnerRecord.runnerPid) $started",
        "Test-BoundProcessAlive $currentProcess.Id $currentStartedAt.AddTicks(1)",
    ] {
        assert!(
            coordinator.contains(required),
            "exact process identity is not enforced or self-tested: {required}"
        );
    }
    assert!(coordinator.contains("$runnerCapture.Process.Id"));
    assert!(coordinator.contains("$runnerCapture.StartedAtUtc"));

    let guard_close_mode = coordinator
        .split("if (args[0] == \"sleeper\" || args[0] == \"control\")")
        .nth(1)
        .unwrap()
        .split("if (args[0] == \"launcher\" && args.Length == 2)")
        .next()
        .unwrap();
    assert!(guard_close_mode.contains("Thread.Sleep(120000)"));
    assert!(
        !guard_close_mode.contains("BindCurrentProcessToKillOnCloseJob"),
        "the pre-transfer guard-close worker must not create its own lifetime Job"
    );
    let guard_close_probe = coordinator
        .split("$guardCloseLauncherProcess = Start-DetachedWorkerProcess")
        .nth(1)
        .unwrap()
        .split("$controlInfo = New-ProcessStartInfo")
        .next()
        .unwrap();
    assert!(guard_close_probe.contains("-Arguments (Join-NativeArguments @(\"sleeper\"))"));
    let retain_exact_guard_worker = guard_close_probe
        .find("$guardCloseBoundProcess = [LbbCoordinator.DetachedWorkerProcess]::OpenExact")
        .unwrap();
    let require_live_guard_worker = guard_close_probe
        .find("$guardCloseLauncherProcess.GuardOwnershipTransferred")
        .unwrap();
    let close_guard = guard_close_probe
        .find("$guardCloseLauncherProcess.Dispose()")
        .unwrap();
    let await_guard_worker = guard_close_probe
        .find("$guardCloseBoundProcess.WaitForExit(5000)")
        .unwrap();
    let reject_live_guard_worker = guard_close_probe
        .find("Test-BoundProcessAlive $guardClosePid $guardCloseStartedAt")
        .unwrap();
    let reopen_guard_stream_exclusively = guard_close_probe.find("[IO.FileShare]::None").unwrap();
    let delete_guard_stream = guard_close_probe
        .find("[IO.File]::Delete($guardCloseLogPath)")
        .unwrap();
    assert!(
        retain_exact_guard_worker < require_live_guard_worker
            && require_live_guard_worker < close_guard
            && close_guard < await_guard_worker
            && await_guard_worker < reject_live_guard_worker,
        "the pre-transfer guard-close probe must retain an exact handle, prove liveness and launcher ownership, dispose without transfer, then prove bounded exact termination"
    );
    assert!(
        reject_live_guard_worker < reopen_guard_stream_exclusively
            && reopen_guard_stream_exclusively < delete_guard_stream,
        "the pre-transfer guard-close probe must prove process termination before proving inherited stream handles closed"
    );
    assert!(
        !guard_close_probe.contains("TransferGuardOwnership()"),
        "the pre-transfer guard-close probe must exercise launcher guard closure"
    );
}

#[test]
fn windows_acceptance_follow_is_non_authoritative_and_summary_bound() {
    let coordinator = fs::read_to_string("scripts/run-windows-computer-use-acceptance.ps1")
        .unwrap()
        .replace("\r\n", "\n");
    let worker = coordinator
        .split("function Invoke-CoordinatorWorker {")
        .nth(1)
        .unwrap()
        .split("function Get-BootstrapFieldNames {")
        .next()
        .unwrap();
    let terminal = worker
        .split("$runnerExit = Complete-CapturedProcess")
        .nth(1)
        .unwrap()
        .split("\n    }\n    catch {")
        .next()
        .unwrap();
    for required in [
        "$summaryPassed = $false",
        "$summary = Read-BoundedJson $summaryPath 1048576 \"Windows acceptance summary\"",
        "$summaryPassed = $summary.passed -is [bool] -and $summary.passed -eq $true",
        "catch { $summaryPassed = $false }",
        "$summaryPassed -and",
        "summaryPassed = $summaryPassed",
        "elseif (-not $summaryPassed) { \"runner-summary-failed\" }",
    ] {
        assert!(
            terminal.contains(required),
            "terminal completion is not bound to a bounded passing summary: {required}"
        );
    }
    let summary_default = terminal.find("$summaryPassed = $false").unwrap();
    let summary_read = terminal.find("$summary = Read-BoundedJson").unwrap();
    let summary_truth = terminal
        .find("$summaryPassed = $summary.passed -is [bool] -and $summary.passed -eq $true")
        .unwrap();
    let completion = terminal.find("$acceptanceCompleted = (").unwrap();
    let final_record = terminal.find("$finalRecord = [pscustomobject]").unwrap();
    let terminal_failure = terminal.find("if (-not $acceptanceCompleted)").unwrap();
    let final_publish = terminal.find("Write-CreateOnceJson $files.Final").unwrap();
    assert!(
        summary_default < summary_read
            && summary_read < summary_truth
            && summary_truth < completion
            && completion < final_record
            && final_record < terminal_failure
            && terminal_failure < final_publish
    );

    let failure_output = coordinator
        .split("function Write-FollowFailureOutput {")
        .nth(1)
        .unwrap()
        .split("function Write-FollowWaitingOutput {")
        .next()
        .unwrap();
    let waiting_output = coordinator
        .split("function Write-FollowWaitingOutput {")
        .nth(1)
        .unwrap()
        .split("function Get-TerminalFollowOutput {")
        .next()
        .unwrap();
    let follow = coordinator
        .split("function Follow-Coordinator {")
        .nth(1)
        .unwrap()
        .split("function Remove-SelfTestStreamFiles {")
        .next()
        .unwrap();
    let chain = coordinator
        .split("function Get-ValidatedFollowChain {")
        .nth(1)
        .unwrap()
        .split("function ConvertTo-ExactPrivateHandoffRecord {")
        .next()
        .unwrap();
    for required in [
        "Assert-ExactStartRecord $start $Config",
        "The coordinator ownership record has no worker predecessor.",
        "The runner intent has no ownership predecessor.",
        "The runner record has no launch-intent predecessor.",
        "The watcher record has no runner predecessor.",
        "The handoff record has no accepted watcher predecessor.",
        "The final record has no runner predecessor.",
    ] {
        assert!(
            chain.contains(required),
            "Follow predecessor-chain validation is missing: {required}"
        );
    }
    let terminal_before_chain = follow
        .find("$terminalOutput = @(Get-TerminalFollowOutput $files $config)")
        .unwrap();
    let nonterminal_chain = follow
        .find("$followChain = Get-ValidatedFollowChain $files $config -SkipFinal")
        .unwrap();
    assert!(
        terminal_before_chain < nonterminal_chain,
        "the first terminal failure must outrank malformed later final state"
    );
    assert!(
        !coordinator.contains("Get-TerminalFollowOutput $files)"),
        "every Follow terminal projection must receive its validated configuration"
    );
    for output in [failure_output, waiting_output] {
        assert!(output.contains("uiActionAllowed = $false"));
        assert!(output.contains("notificationOnly = $true"));
        assert!(output.contains("acceptedAsAuthority = $false"));
    }
    assert!(follow.contains("uiActionAllowed = $false"));
    assert!(follow.contains("notificationOnly = [bool]$handoff.notificationOnly"));
    assert!(follow.contains("acceptedAsAuthority = [bool]$handoff.acceptedAsAuthority"));
    for required in [
        "requiresSeparateAuthorization = [bool]$handoff.requiresSeparateAuthorization",
        "markerGrantsAuthorization = [bool]$handoff.markerGrantsAuthorization",
        "markerGrantsConsent = [bool]$handoff.markerGrantsConsent",
        "externalOneShotConsentRequired = [bool]$handoff.externalOneShotConsentRequired",
        "externalAuthorizationVerifiedByWatcher = [bool]$handoff.externalAuthorizationVerifiedByWatcher",
        "consumerMustDeduplicateByRequestId = $true",
        "uiActionAllowed = $false",
        "notificationOnly = [bool]$handoff.notificationOnly",
        "acceptedAsAuthority = [bool]$handoff.acceptedAsAuthority",
    ] {
        assert!(
            follow.contains(required),
            "Follow handoff is not explicitly non-authoritative: {required}"
        );
    }
    for forbidden in [
        "Write-CreateOnceJson",
        "Start-CapturedProcess",
        "Start-DetachedWorkerProcess",
        ".Kill()",
        "SetEnvironmentVariable",
        "[IO.File]::Write",
        "[IO.File]::Move",
        "[IO.File]::Delete",
    ] {
        assert!(
            !follow.contains(forbidden),
            "Follow must remain a read-only notification projection: {forbidden}"
        );
    }
    let terminal_projection = coordinator
        .split("function Get-TerminalFollowOutput {")
        .nth(1)
        .unwrap();
    let terminal_projection = terminal_projection
        .split("function Follow-Coordinator {")
        .next()
        .unwrap();
    assert!(
        !terminal_projection.contains("return $null"),
        "an absent terminal record must emit no pipeline object"
    );
    let failure_check = terminal_projection
        .find("if ([IO.File]::Exists($Files.Failure))")
        .unwrap();
    let final_check = terminal_projection
        .find("\n    if (-not [IO.File]::Exists($Files.Final)) { return }")
        .unwrap();
    assert!(failure_check < final_check);
    let failure_branch = &terminal_projection[failure_check..final_check];
    assert!(failure_branch.contains("Write-FollowFailureOutput"));
    assert!(failure_branch.contains("return"));
    assert!(terminal_projection.contains("[int]$final.exitCode -ne 0"));
    assert!(terminal_projection.contains("$final.summaryPresent -ne $true"));
    assert!(terminal_projection.contains("$final.summaryPassed -ne $true"));
    assert!(terminal_projection.contains("$final.evidenceDirectoryPresent -ne $true"));
    assert!(terminal_projection.contains("-ReasonCode \"final-record-not-successful\""));
    for required in [
        "Follow non-authoritative repeated-handoff self-test failed.",
        "Follow terminal-failure precedence self-test failed.",
        "Follow lone-final chain self-test accepted an impossible completion.",
        "Follow missing-intent chain self-test accepted an impossible handoff.",
        "$final.summaryPassed -ne $true",
    ] {
        assert!(
            coordinator.contains(required),
            "Follow failure precedence or summary binding is not self-tested: {required}"
        );
    }
}

#[test]
fn windows_foreground_arm_handoff_watcher_is_strict_read_only_and_non_authoritative() {
    let watcher = fs::read_to_string("scripts/wait-windows-foreground-arm-handoff.ps1")
        .unwrap()
        .replace("\r\n", "\n");
    let runner = fs::read_to_string("scripts/test-windows-computer-use.ps1")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "$script:ProductVersion = \"0.12.59\"",
        "$script:MarkerSchemaVersion = 2",
        "function Assert-ExactPropertyOrder {",
        "function Assert-ExactMarkerSchema {",
        "function Resolve-OrdinaryEvidenceDirectory {",
        "function Read-AtomicRequestMarker {",
        "function Assert-BoundRunnerState {",
        "function Assert-FreshMarkerBinding {",
        "function New-SanitizedHandoff {",
        "function Wait-ForegroundArmHandoff {",
        "[object[]]$MarkerReaderArguments = @()",
        "$atomicMarker = & $MarkerReader @MarkerReaderArguments",
        "-MarkerReaderArguments @($markerPath, $operatorDirectory)",
        "lbb-foreground-arm-watcher-self-test-",
        "The zero-write live-style marker callback failed its self-test.",
        "preferredRelaySurface = \"windows-computer-use-app-share\"",
        "fallbackRelaySurface = \"human-on-windows-session\"",
        "expectedAccessibleName = \"Click to arm Windows acceptance\"",
        "externalAuthorizationVerifiedByWatcher = $false",
        "markerGrantsAuthorization = $false",
        "markerGrantsConsent = $false",
        "externalOneShotConsentRequired = $true",
        "visualConfirmationRequired = $true",
        "retryOnUnknownOutcome = $false",
        "runnerIdentityMatched = $true",
        "markerFresh = $true",
        "processIdentifiersRecorded = $false",
        "pathsRecorded = $false",
        "secretsRecorded = $false",
        "The marker predates the bound runner instance.",
        "The marker publication time is unacceptably far in the future.",
        "The marker has expired.",
        "The marker is stale for immediate operator handoff.",
        "The bound Windows acceptance runner is not alive.",
        "The live runner PID does not match the exact expected start time.",
        "Windows foreground-arm handoff watcher self-test passed.",
        "Write-Output ($handoff | ConvertTo-Json -Depth 8 -Compress)",
    ] {
        assert!(
            watcher.contains(required),
            "Windows foreground-arm handoff watcher is missing: {required}"
        );
    }

    for field in [
        "schemaVersion",
        "productVersion",
        "kind",
        "status",
        "requestId",
        "publishedAtUtc",
        "timeoutSeconds",
        "operatorActionRequired",
        "preferredRelaySurface",
        "fallbackRelaySurface",
        "expectedVisibleWindowTitle",
        "expectedVisibleButtonText",
        "expectedAccessibleName",
        "action",
        "stopUiAfterAction",
        "requiresSeparateAuthorization",
        "markerGrantsAuthorization",
        "markerGrantsConsent",
        "externalOneShotConsentRequired",
        "visualConfirmationRequired",
        "maximumClickAttempts",
        "retryOnUnknownOutcome",
        "instruction",
        "requestDelivered",
        "buttonEnabled",
        "nativeTopologyMatched",
        "inputStateAtPublication",
        "notificationOnly",
        "acceptedAsAuthority",
        "rawWindowHandlesRecorded",
        "rawCursorCoordinatesRecorded",
        "pathsRecorded",
        "secretsRecorded",
    ] {
        assert!(
            watcher.contains(&format!("    \"{field}\","))
                || watcher.contains(&format!("    \"{field}\"\n")),
            "watcher exact request schema omits {field}"
        );
    }

    for forbidden in [
        "SetForegroundWindow",
        "AttachThreadInput",
        "SendInput",
        "mouse_event",
        "SetCursorPos",
        "PostMessage",
        "SendMessage",
        "PerformClick",
        "Invoke-RestMethod",
        "Invoke-WebRequest",
        "Start-Process",
        "[IO.File]::WriteAllText",
        "[IO.File]::WriteAllBytes",
        "[IO.FileMode]::Create",
        "[IO.FileMode]::CreateNew",
        "[IO.FileMode]::Append",
        "[IO.FileAccess]::Write",
    ] {
        assert!(
            !watcher.contains(forbidden),
            "the read-only handoff watcher contains a UI, product, or file-write primitive: {forbidden}"
        );
    }

    let handoff_factory = watcher
        .split("function New-SanitizedHandoff {")
        .nth(1)
        .unwrap()
        .split("function Wait-ForegroundArmHandoff {")
        .next()
        .unwrap();
    for forbidden in ["EvidenceDirectory", "RunnerProcessId", "RunnerStartedAtUtc"] {
        assert!(
            !handoff_factory.contains(forbidden),
            "sanitized handoff leaks coordinator identity: {forbidden}"
        );
    }

    assert_eq!(watcher.matches("Write-Output ($handoff").count(), 1);
    assert!(
        watcher.contains("PowerShell 7 runs a\n    # GetNewClosure() block in a dynamic module")
    );
    assert!(!watcher.contains("}.GetNewClosure() `\n        -ExpectedText"));
    let live_mode = watcher
        .split("if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {")
        .nth(1)
        .expect("watcher is missing its live-mode boundary");
    assert!(live_mode.contains(
        "$markerReader = {\n    param([string]$boundMarkerPath, [string]$boundOperatorDirectory)\n    return Read-AtomicRequestMarker $boundMarkerPath $boundOperatorDirectory\n}"
    ));
    assert!(
        !live_mode.contains(".GetNewClosure()"),
        "the production watcher must not move its marker callback into a dynamic module"
    );
    assert!(!watcher.contains(
        "$markerReader = { return Read-AtomicRequestMarker $markerPath $operatorDirectory }.GetNewClosure()"
    ));
    assert!(runner.contains("-ProductVersion $Version"));
    assert!(runner.contains("-ProductVersion \"0.12.59\""));
    assert!(runner.contains("maximumClickAttempts -ne 1"));
    assert!(runner.contains("maximumClickAttempts -ne 0"));
}

#[test]
fn vendored_screen_capture_patch_preserves_macos_13_resize_support() {
    let manifest = fs::read_to_string("Cargo.toml").unwrap();
    let vendor_manifest = fs::read_to_string("vendor/screencapturekit-8.0.1/Cargo.toml").unwrap();
    let bridge = fs::read_to_string(
        "vendor/screencapturekit-8.0.1/swift-bridge/Sources/ScreenCaptureKitBridge/Stream.swift",
    )
    .unwrap();
    let patch_policy =
        fs::read_to_string("vendor/screencapturekit-8.0.1/LOCAL_PATCHES.md").unwrap();

    assert!(manifest.contains("screencapturekit = { path = \"vendor/screencapturekit-8.0.1\" }"));
    assert!(vendor_manifest.contains("version = \"8.0.1\""));
    assert!(bridge.contains("if #available(macOS 13.0, *)"));
    assert!(bridge.contains("updateConfiguration requires macOS 13.0 or later"));
    assert!(!bridge.contains("updateConfiguration requires macOS 14.0 or later"));
    assert!(patch_policy.contains("2a9f13bcbeadb0aabc5596f0ff3d2ba71da8c1d0"));
    assert!(
        patch_policy.contains("9ddaa8d6b16a2762c9a97c9a6297f04cb8ded0487e5ef02dc98b4e2bee3a26c7")
    );
    assert!(patch_policy.contains("Apple exposes the underlying API from macOS 12.3"));
}

#[test]
fn semantic_backends_revalidate_exact_targets_and_report_effects() {
    let macos = fs::read_to_string("src/computer/ax_macos.rs").unwrap();
    let windows = fs::read_to_string("src/computer/uia_windows.rs").unwrap();
    let macos_platform = fs::read_to_string("src/computer/platform_macos.rs").unwrap();
    let windows_platform = fs::read_to_string("src/computer/platform_windows.rs").unwrap();
    let computer = fs::read_to_string("src/computer.rs").unwrap();
    let server = fs::read_to_string("src/server.rs").unwrap();
    assert!(macos.contains("_AXUIElementGetWindow"));
    assert!(macos.contains("resolve_verified"));
    assert!(macos.contains("AXUIElementPerformAction"));
    assert!(macos.contains("begin_side_effect(\"macOS AX action dispatch\")"));
    assert!(macos.contains("begin_side_effect(\"macOS AX value dispatch\")"));
    assert!(macos.contains("target-window-closed"));
    assert!(macos.contains("masked-length-confirmed"));
    assert!(macos.contains("AXSubrole"));
    assert!(macos.contains("AXSecureTextField"));
    assert!(macos.contains("let value = if sensitive"));
    assert!(windows.contains("ElementFromHandle"));
    assert!(windows.contains("resolve_verified"));
    assert!(windows.contains("IUIAutomationInvokePattern"));
    assert!(windows.contains("IUIAutomationValuePattern"));
    assert!(windows.contains("begin_side_effect(\"UI Automation Invoke dispatch\")"));
    assert!(windows.contains("begin_side_effect(\"UI Automation SetValue dispatch\")"));
    assert!(windows.contains("CurrentIsPassword"));
    assert!(windows.contains("fail_closed_password_state"));
    assert!(windows.contains("COMPUTER_POSTCONDITION_FAILED"));
    assert!(computer.contains("pub sensitive: bool"));
    assert!(computer.contains("pub value_redacted: bool"));
    assert!(server.contains("let value_redacted = sensitive"));

    for platform in [&macos_platform, &windows_platform] {
        let invoke = platform.split("pub fn invoke(").nth(1).unwrap();
        let invoke = invoke.split("pub fn set_value(").next().unwrap();
        assert!(invoke.contains("Result<(serde_json::Value, InvariantReport), ComputerError>"));
        let set_value = platform.split("pub fn set_value(").nth(1).unwrap();
        let set_value = set_value.split("pub fn limitations(").next().unwrap();
        assert!(set_value.contains("Result<(serde_json::Value, InvariantReport), ComputerError>"));
    }
    assert!(windows_platform.contains("Ok((backend_effect, report))"));
    let macos_semantic_finish = macos_platform
        .split("fn finish_semantic_after_snapshot(")
        .nth(1)
        .unwrap()
        .split("fn mouse_event(")
        .next()
        .unwrap();
    assert!(macos_semantic_finish.contains("let report = report.assert_held(stage)?;"));
    assert!(macos_semantic_finish.contains("Ok((backend_effect?, report))"));
    assert!(
        macos_semantic_finish
            .find("report.assert_held(stage)?")
            .unwrap()
            < macos_semantic_finish.find("backend_effect?").unwrap()
    );

    let invoke = computer.split("    fn invoke(").nth(1).unwrap();
    let invoke = invoke.split("    fn set_value(").next().unwrap();
    assert!(invoke.contains("evidence.extend(invariant_evidence(&invariants))"));
    assert!(invoke.contains("\"invariants\": invariants"));
    let set_value = computer.split("    fn set_value(").nth(1).unwrap();
    let set_value = set_value.split("    fn point(").next().unwrap();
    assert!(set_value.contains("evidence.extend(invariant_evidence(&invariants))"));
    assert!(set_value.contains("\"invariants\": invariants"));

    let signature = windows.split("unsafe fn signature").nth(1).unwrap();
    assert!(
        signature.find("sensitive_uia_element").unwrap()
            < signature.find("UIA_ValuePatternId").unwrap()
    );
}

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
fn withdrawn_v0_12_59_windows_image_path_mismatch_is_byte_exact_and_fail_closed() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.59/computer/attempts/withdrawn-ece060c-windows-helper-readiness-image-mismatch",
    );
    let expected = [
        (
            "README.md",
            1_967,
            "a9eceb92da81f5bf4eff337e9381155c3b4b2ffb4a81a2e8a24fe3bf06af776e",
        ),
        (
            "fixture/fixture-events.ndjson",
            869,
            "ce94da3b5668970438598356ec2bbca683dd2d0454692737f6604d2c5f7360af",
        ),
        (
            "fixture/fixture-ready.json",
            208,
            "2bea0ed2f3cef3fa5e69273840d9dead43949ee16bd3eb8b35c207a0542df6cd",
        ),
        (
            "fixture/fixture-state.json",
            1_326,
            "9da653e8d9c7026aec4a5760c1a1f713f0680bd2b29e6b1298cfa132be30fd6b",
        ),
        (
            "summary.json",
            15_799,
            "b2843f302c1b19ed55c99e926da95e9b23dced1f4f17d04f3e2c912803d5f873",
        ),
        (
            "terminal-failure.json",
            311,
            "f1bf8fb3193c2726d88929281c6157756349c6ad6e47dd4ce25775cf3d4c0e16",
        ),
    ];
    for (name, expected_bytes, expected_sha256) in expected {
        let path = attempt_root.join(name);
        assert_eq!(fs::metadata(&path).unwrap().len(), expected_bytes);
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            expected_sha256
        );
    }

    let summary: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(attempt_root.join("summary.json")).unwrap())
            .unwrap();
    assert_eq!(summary["passed"], false);
    assert_eq!(
        summary["failureDetails"]["stage"],
        "bind-initial-helper-readiness"
    );
    assert_eq!(summary["helperTopologyPollCount"], 273);
    assert_eq!(summary["helperTopologyLastObservation"]["connected"], true);
    assert_eq!(
        summary["helperTopologyLastObservation"]["helloStateMatched"],
        true
    );
    assert_eq!(
        summary["helperTopologyLastObservation"]["controllerProcessMatched"],
        true
    );
    assert_eq!(
        summary["helperTopologyLastObservation"]["workerImageMatched"],
        false
    );
    assert_eq!(
        summary["releaseCandidateBinding"]["artifactZipSha256"],
        "ece060c68be4e8e8ef49de6ef6c7d52ab69082a37e2fcf41c5c0e11e4e3f1313"
    );

    let terminal: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("terminal-failure.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(terminal["status"], "failed-closed");
    assert_eq!(terminal["reasonCode"], "runner-summary-failed");
    assert_eq!(terminal["retryAllowed"], false);

    let retained_text = [
        fs::read_to_string(attempt_root.join("README.md")).unwrap(),
        fs::read_to_string(attempt_root.join("summary.json")).unwrap(),
        fs::read_to_string(attempt_root.join("terminal-failure.json")).unwrap(),
    ]
    .join("\n");
    for forbidden in ["C:\\Users\\", "/Users/", "GH_TOKEN=", "ghp_", "github_pat_"] {
        assert!(!retained_text.contains(forbidden));
    }
}

#[test]
fn withdrawn_v0_12_60_windows_wgc_frame_age_failure_is_byte_exact_and_fail_closed() {
    fn collect_files(
        root: &std::path::Path,
        directory: &std::path::Path,
        files: &mut BTreeSet<String>,
    ) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
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

    let attempt_root = std::path::Path::new(
        "evidence/v0.12.60/computer/attempts/withdrawn-7ceb294-windows-wgc-compositor-frame-age",
    );
    let expected = [
        (
            "README.md",
            2_131,
            "e0bf7c4f29d4b246305c4c6ec588d74a171ab4ea548c8ffe42970b84c15691c3",
        ),
        (
            "fixture/fixture-events.ndjson",
            1_502,
            "3c3a6e11b9a13a4258b9c3d0b7831641d4b30bb0695893baaf8bfc51c5045ae6",
        ),
        (
            "fixture/fixture-ready.json",
            207,
            "46b32ac2de7b8d9ee827a097efe1a3295f4989ea16e47cc04823cb076a9dc18d",
        ),
        (
            "fixture/fixture-state.json",
            1_342,
            "b56138602b7c4798c02c74c616d62bda1b3a04141593bec11f91b4efc296e56d",
        ),
        (
            "operator/foreground-arm-received.json",
            697,
            "afb7c66e22d8940918f36a0e7957fd6f117b1f3079358c404954b8e7d507c9be",
        ),
        (
            "operator/foreground-arm-request.json",
            1_541,
            "c9f645f0e1661c456dedd58b5933527922701ddec5f260f91391de9f5df6d0c6",
        ),
        (
            "steps/01-protocol-bound-helper-readiness.json",
            8_750,
            "ebce4e83066a204fd622dc82abdf504a40b8cf6f8e1bc0d948bc72df64897170",
        ),
        (
            "steps/02-foreground-arm-request-delivery.json",
            597,
            "4bb7c4324d0825f4ad01e28e4a6f868da328d45107aea1763a2628307726f067",
        ),
        (
            "steps/03-foreground-arm-proof.json",
            1_538,
            "0fdf2ec50e18b4970c4bbec9112a70c128beb58ec0feb7240457fd94165068ac",
        ),
        (
            "steps/04-post-arm-protocol-bound-helper-continuity.json",
            8_751,
            "b032a28e7036e1aa482b9fac6a0347c51cf0724b5d3f0739ac9d081436d200cc",
        ),
        (
            "summary.json",
            21_544,
            "ad9fe76ab754937e13b773c61546e9970f2e2c072bfb493b38b86d6a7585f165",
        ),
        (
            "terminal-failure.json",
            311,
            "327212c7d829960f5426f4726b9e1afbdf052ddd2d18ccfaecb0ab7b2306a63c",
        ),
    ];
    let mut actual = BTreeSet::new();
    collect_files(attempt_root, attempt_root, &mut actual);
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

    let summary: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(attempt_root.join("summary.json")).unwrap())
            .unwrap();
    assert_eq!(summary["passed"], false);
    assert_eq!(
        summary["failureDetails"]["stage"],
        "baseline-status-and-observation"
    );
    assert_eq!(
        summary["failure"],
        "Loopback command computer.observe returned COMPUTER_CAPTURE_FAILED: WGC compositor frame age exceeded the monotonic range"
    );
    assert_eq!(summary["helperTopologyChecks"].as_array().unwrap().len(), 2);
    assert!(
        summary["helperTopologyChecks"]
            .as_array()
            .unwrap()
            .iter()
            .all(|check| check["exactImageMatched"] == true
                && check["directChildEnumerated"] == true
                && check["protocolRoundTrip"] == true)
    );
    assert_eq!(summary["foregroundArmProof"]["completed"], true);
    assert_eq!(summary["foregroundArmProof"]["fixtureRequestCount"], 1);
    assert_eq!(
        summary["foregroundArmProof"]["fixtureAcknowledgementCount"],
        1
    );
    assert_eq!(
        summary["foregroundArmProof"]["fixtureLeftMouseDownCount"],
        1
    );
    assert_eq!(summary["foregroundArmProof"]["fixtureLeftMouseUpCount"], 1);

    let terminal: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("terminal-failure.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(terminal["status"], "failed-closed");
    assert_eq!(terminal["reasonCode"], "runner-summary-failed");
    assert_eq!(terminal["retryAllowed"], false);

    let retained_text = [
        fs::read_to_string(attempt_root.join("README.md")).unwrap(),
        fs::read_to_string(attempt_root.join("summary.json")).unwrap(),
        fs::read_to_string(attempt_root.join("terminal-failure.json")).unwrap(),
    ]
    .join("\n");
    for boundary in [
        "was reserved and executed on 2026-08-29",
        "candidate must not be retried or resumed",
        "stock-Chrome claim",
        "It contains no candidate bytes, credentials, absolute paths",
    ] {
        assert!(
            retained_text.contains(boundary),
            "withdrawn v0.12.60 record omits boundary: {boundary}"
        );
    }
    for forbidden in ["C:\\Users\\", "/Users/", "GH_TOKEN=", "ghp_", "github_pat_"] {
        assert!(!retained_text.contains(forbidden));
    }
}

#[test]
fn withdrawn_v0_12_61_windows_wgc_timestamp_ahead_is_byte_exact_and_fail_closed() {
    fn collect_files(
        root: &std::path::Path,
        directory: &std::path::Path,
        files: &mut BTreeSet<String>,
    ) {
        for entry in fs::read_dir(directory).unwrap() {
            let entry = entry.unwrap();
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

    let attempt_root = std::path::Path::new(
        "evidence/v0.12.61/computer/attempts/withdrawn-619e043-windows-wgc-compositor-timestamp-ahead",
    );
    let expected = [
        (
            "README.md",
            2_092,
            "d54f93b7ec0c2135f31eca972fe0fc480b905537f4cbaf7331236be4c776991e",
        ),
        (
            "fixture/fixture-events.ndjson",
            1_504,
            "7de5a0c2ac375d41a3002ab335251788a0dde3212e4ffa7d69deb33a85d8fa6c",
        ),
        (
            "fixture/fixture-ready.json",
            205,
            "a9a5ee18345437cf987c173b44d282eec778d92d5512357c3fbc30f631508ac4",
        ),
        (
            "fixture/fixture-state.json",
            1_342,
            "3dcf83cb4b497377e8405d636acf5d4a32c2b03612218c6b5da54504c671cd0f",
        ),
        (
            "operator/foreground-arm-received.json",
            697,
            "c78609e36ed7ce3e6bf9dcb3b1ebcee09f050e69343d12e3ee366c316ff94618",
        ),
        (
            "operator/foreground-arm-request.json",
            1_541,
            "40af3d5371096b824fbd2684e6382cc4d858d905959f42c184ea29b7a48c0c83",
        ),
        (
            "steps/01-protocol-bound-helper-readiness.json",
            8_752,
            "161d589b5082f1e39ab87b22d70ead562cd5447d81e3679b6db1da0a15d7afe2",
        ),
        (
            "steps/02-foreground-arm-request-delivery.json",
            598,
            "bd2e1c4ab42ed708e122ac686e8393924ab69cf68c8e5d0462e86207f4d3e4fc",
        ),
        (
            "steps/03-foreground-arm-proof.json",
            1_541,
            "a3203defacfe43e7b82071847c4834ad4797412522a8039be1541f67a30fe5b3",
        ),
        (
            "steps/04-post-arm-protocol-bound-helper-continuity.json",
            8_753,
            "f476dac445a45e0e9b4433748ee1efc6db45d422683b915ca327ad9cb8e9585c",
        ),
        (
            "summary.json",
            21_572,
            "d00dc31ee2c953278bad19f32494df9929f774020b324501e7c85425d98728dd",
        ),
        (
            "terminal-failure.json",
            311,
            "4f2aa3bcdab43799a09ddde7f0e6b6a36e9eba3da9858f0636a88e061be1cbf5",
        ),
    ];
    let mut actual = BTreeSet::new();
    collect_files(attempt_root, attempt_root, &mut actual);
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

    let summary: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(attempt_root.join("summary.json")).unwrap())
            .unwrap();
    assert_eq!(summary["passed"], false);
    assert_eq!(
        summary["failureDetails"]["stage"],
        "baseline-status-and-observation"
    );
    assert_eq!(
        summary["failure"],
        "Loopback command computer.observe returned COMPUTER_CAPTURE_FAILED: The WGC compositor timestamp is ahead of the monotonic clock"
    );
    assert_eq!(summary["foregroundArmProof"]["completed"], true);
    assert_eq!(summary["foregroundArmProof"]["fixtureRequestCount"], 1);
    assert_eq!(
        summary["foregroundArmProof"]["fixtureAcknowledgementCount"],
        1
    );
    assert_eq!(
        summary["foregroundArmProof"]["fixtureLeftMouseDownCount"],
        1
    );
    assert_eq!(summary["foregroundArmProof"]["fixtureLeftMouseUpCount"], 1);
    assert_eq!(summary["helperTopologyChecks"].as_array().unwrap().len(), 2);
    assert_eq!(summary["cleanupIssues"].as_array().unwrap().len(), 0);
    assert_eq!(summary["tokenPersisted"], false);

    let terminal: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(attempt_root.join("terminal-failure.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(terminal["status"], "failed-closed");
    assert_eq!(terminal["reasonCode"], "runner-summary-failed");
    assert_eq!(terminal["retryAllowed"], false);

    let retained_text = [
        fs::read_to_string(attempt_root.join("README.md")).unwrap(),
        fs::read_to_string(attempt_root.join("summary.json")).unwrap(),
        fs::read_to_string(attempt_root.join("terminal-failure.json")).unwrap(),
    ]
    .join("\n");
    for boundary in [
        "was reserved and executed on 2026-08-29",
        "candidate must not be retried or resumed",
        "suite step or stock-Chrome acceptance ran",
        "It contains no candidate bytes, credentials, absolute paths",
    ] {
        assert!(
            retained_text.contains(boundary),
            "withdrawn v0.12.61 record omits boundary: {boundary}"
        );
    }
    for forbidden in ["C:\\Users\\", "/Users/", "GH_TOKEN=", "ghp_", "github_pat_"] {
        assert!(!retained_text.contains(forbidden));
    }
}

#[test]
fn withdrawn_v0_12_62_windows_reservation_outcome_unknown_is_byte_exact_and_fail_closed() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.62/computer/attempts/withdrawn-6a5f396-windows-reservation-outcome-unknown",
    );
    let expected = [
        (
            "README.md",
            1_562,
            "d836bf0d49e951e6776e8942607fbb8f7a99acb7a6035d9cc5b5926bfa4fb59b",
        ),
        (
            "reservation.json",
            469,
            "a0b90639cc0de591058dfb8f9fa808a84e179b214f0c85e6036a9dfcedc2c997",
        ),
    ];
    let actual = fs::read_dir(attempt_root)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            assert!(entry.file_type().unwrap().is_file());
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

    let reservation: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(attempt_root.join("reservation.json")).unwrap())
            .unwrap();
    assert_eq!(reservation["schemaVersion"], 2);
    assert_eq!(
        reservation["kind"],
        "windows-acceptance-attempt-reservation"
    );
    assert_eq!(reservation["status"], "reserved-no-retry");
    assert_eq!(reservation["productVersion"], "0.12.62");
    assert_eq!(reservation["retryAllowed"], false);
    assert_eq!(reservation["pathsRecorded"], false);
    assert_eq!(reservation["secretsRecorded"], false);

    let retained_text = [
        fs::read_to_string(attempt_root.join("README.md")).unwrap(),
        fs::read_to_string(attempt_root.join("reservation.json")).unwrap(),
    ]
    .join("\n");
    for boundary in [
        "candidate-execution-unknown",
        "must therefore never be retried, resumed, or published",
        "zero matching v0.12.62 coordinator directories",
        "It contains no candidate bytes, credentials, absolute paths",
    ] {
        assert!(
            retained_text.contains(boundary),
            "withdrawn v0.12.62 record omits boundary: {boundary}"
        );
    }
    for forbidden in ["C:\\Users\\", "/Users/", "GH_TOKEN=", "ghp_", "github_pat_"] {
        assert!(!retained_text.contains(forbidden));
    }
}

#[test]
fn withdrawn_v0_12_63_macos_share_pump_stale_frame_is_byte_exact_and_fail_closed() {
    let attempt_root = std::path::Path::new(
        "evidence/v0.12.63/computer/attempts/withdrawn-7cfc4f0-macos-app-share-stale-frame",
    );
    let expected = [
        (
            "README.md",
            1_818,
            "26f83353b3c93f5b85742c491355b88c1ea5f2dd415aa6106265c3b5046d303c",
        ),
        (
            "deliberate-concurrency/computer-01-exact-window-observe.png",
            836_031,
            "250dfe7fdc9d64f99f40960cf8295868fc70526257cf091e36e98c404d0c3a15",
        ),
        (
            "deliberate-concurrency/computer-02-semantic-set-value.png",
            850_885,
            "66f79accfc92aed6db04a1ce24c8f681c782ead3b8a6c44ff026bdf3b9028931",
        ),
        (
            "deliberate-concurrency/computer-03-semantic-invoke.png",
            851_837,
            "0d1d657e915b2496c9fc1c7ec48eb0d448337b676699ce0ff8b2187634f4323f",
        ),
        (
            "deliberate-concurrency/computer-04-persistent-scstream-start.png",
            786_668,
            "f6416a529ac949b08a9bc78190eb6a39740256b272e488d44c0da3dc0ade021a",
        ),
        (
            "deliberate-concurrency/computer-05-live-share-pixel-action.png",
            788_383,
            "776132a0921abd1d64f408e247f1b1ee0b9ccbc8af1a2046373bf30acea99a4d",
        ),
        (
            "deliberate-concurrency/computer-06-persistent-share-resize.png",
            715_366,
            "babaab5e6c33891eb06ca8850324871a83552529d94c0f9705c25adab133fc05",
        ),
        (
            "deliberate-concurrency/helper-results.json",
            31_376,
            "ca36556409fb147f0c332575e524e6800f0899fa964f5bf0eb0d926898b1da89",
        ),
        (
            "deliberate-concurrency/helper-rig.log",
            18_076,
            "d4b947c34ce30be3166bb602a484e47fcc98d8952270dcb566bbcf0c2d9dffe1",
        ),
        (
            "deliberate-concurrency/operator/macos-app-share-concurrency-handoff-request.json",
            799,
            "136676443be7b81b6b6dad3a3ac065907dc5c9f0388095ef1f44eca15a72cd1a",
        ),
        (
            "deliberate-concurrency/operator/macos-app-share-concurrency-handoff-start.json",
            443,
            "f5117eb70d22966f44b7dfb56f9565ec090c113700104cd6bb950df702851d6d",
        ),
        (
            "quiet/computer-01-exact-window-observe.png",
            832_000,
            "ed0570f2f49e4720e06d5801d2936f17f804d1826dc3733f3d7a4c3054f9a330",
        ),
        (
            "quiet/computer-02-semantic-set-value.png",
            847_279,
            "2ad6d57a25b129a98919daba065e5028dc313e1ca9a3f46b7dbd43df4bb867e0",
        ),
        (
            "quiet/computer-03-semantic-invoke.png",
            847_585,
            "d7eef684ffb97e78b98b4ed9828e9b0a2ff002884fdc13ceb51488ed9bb3f76e",
        ),
        (
            "quiet/computer-04-persistent-scstream-start.png",
            781_853,
            "8c4cfaf90873e78a0caad1f9163ac690331c6d2858d4656bf0e15dbc0aff370e",
        ),
        (
            "quiet/computer-05-live-share-pixel-action.png",
            782_895,
            "52a063c4e351cf9b6c990dd18d629414f8b9e8069a0d978e3798170e49c1949c",
        ),
        (
            "quiet/computer-06-persistent-share-resize.png",
            710_575,
            "8d895c6650741e848cf30cc23e6e7d50dbce4ee154d7d23363e2c48c5feefdb1",
        ),
        (
            "quiet/helper-results.json",
            125_479,
            "a7ae4eaf4fc5d26604e823b0676db1436c3989484ad72d3db7dd68fef5e80cb1",
        ),
        (
            "quiet/helper-rig.log",
            36_866,
            "6442ed2ac338810ed5042757e23c7a0d7e1dc13282fc777bf96950d48617beb3",
        ),
    ];
    assert_eq!(
        walk_files(attempt_root),
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
    assert_eq!(quiet["productVersion"], "0.12.63");
    assert_eq!(quiet["status"], "passed-release-candidate");
    assert_eq!(quiet["assertions"]["passed"], 207);
    assert_eq!(deliberate["productVersion"], "0.12.63");
    assert_eq!(deliberate["status"], "failed-release-candidate");
    assert_eq!(deliberate["assertions"]["passed"], 93);
    assert_eq!(
        deliberate["fatal"],
        "computer.click returned HTTP 409: COMPUTER_STALE_FRAME"
    );
    assert_eq!(
        deliberate["failureDiagnostics"]["stage"],
        "postResizePixelAction"
    );
    assert!(deliberate["failureDiagnostics"]["actionDispatched"].is_null());
    assert_eq!(
        deliberate["failureDiagnostics"]["systemProbe"]["equality"]["sharedInputSeatActivityObserved"],
        false
    );
    assert_eq!(
        deliberate["appShareHandoff"]["startReceiptAcknowledged"],
        true
    );
    assert_eq!(
        deliberate["appShareHandoff"]["authorityFreshAtDispatch"],
        true
    );
    assert_eq!(
        deliberate["appShareHandoff"]["targetPostconditionObserved"],
        false
    );
    assert_eq!(
        deliberate["releaseCandidateBinding"]["artifactZipSha256"],
        "7cfc4f040470bee29c16f384b46f4994a09cf4553cd062a8fb206d2aa1f351f3"
    );
    let retained = [
        fs::read_to_string(attempt_root.join("README.md")).unwrap(),
        fs::read_to_string(attempt_root.join("deliberate-concurrency/helper-results.json"))
            .unwrap(),
        fs::read_to_string(attempt_root.join("deliberate-concurrency/helper-rig.log")).unwrap(),
    ]
    .join("\n");
    for boundary in [
        "This is terminal negative evidence, not release evidence.",
        "The candidate was not retried.",
        "No completion receipt exists.",
        "Version 0.12.64 tightens the post-handoff frame-authority handoff",
    ] {
        assert!(retained.contains(boundary));
    }
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
fn windows_wgc_frame_age_preserves_past_age_and_saturates_future_leads() {
    let source = fs::read_to_string("src/computer/share_windows.rs").unwrap();
    for required in [
        "let age_scaled = current_scaled",
        ".checked_sub(frame_scaled)",
        "if age_scaled <= 0",
        "return Ok(Duration::ZERO);",
        "Round upward so the conversion never understates frame age.",
        "qpc_frame_age(i64::MAX, 0, 1).unwrap()",
        "future compositor timestamps must saturate to receipt time",
    ] {
        assert!(
            source.contains(required),
            "Windows WGC frame-age source lost required behavior: {required}"
        );
    }
    for forbidden in [
        "future_tolerance",
        "The WGC compositor timestamp is ahead of the monotonic clock",
    ] {
        assert!(
            !source.contains(forbidden),
            "Windows WGC frame-age source retained obsolete future rejection: {forbidden}"
        );
    }
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

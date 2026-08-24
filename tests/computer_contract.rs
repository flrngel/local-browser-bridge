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
        .split("fn post_mouse(")
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
fn macos_v0_12_14_pointer_evidence_is_bounded_corroboration_not_causal_attribution() {
    let rig = fs::read_to_string("evidence/v0.12.19/computer/helper-evidence-rig.mjs").unwrap();
    let probe = fs::read_to_string("evidence/v0.12.19/computer/SystemProbe.swift").unwrap();
    assert!(rig.contains("failureProbeBaseline"));
    assert!(rig.contains("collectFailureDiagnostics"));
    assert!(rig.contains("systemInvariants(failureProbeBaseline.system, after)"));
    assert!(rig.contains("fixtureCounterSnapshot"));
    assert!(rig.contains("semanticValueMatchesExpected"));
    assert!(rig.contains("failureDiagnostics,"));
    for required in [
        "schemaVersion: 6",
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
        "ACTION REQUIRED: the orange macOS pointer handoff panel is visible",
        "const DELIBERATE_CONCURRENCY_ACTION_MS = 2_000",
        "const DELIBERATE_CONCURRENCY_WAIT_MS = 300_000",
        "const POINTER_HANDOFF_COMPLETION_GRACE_MS = 10_000",
        "function clickFreePointerMotionProgress(before, after)",
        "if (field !== \"mouseMoved\") return \"disallowed\"",
        "let pointerHandoffClickFreeCompletionObserved = false",
        "const completionPresentationProgress = clickFreePointerMotionProgress(\n    completionBoundary,\n    completePromptSample,\n  )",
        "pointerHandoffClickFreeCompletionObserved = true",
        "clickFreeMotionObserved:",
        "deliberate concurrency crossed both product and independent action boundaries",
        "state === \"concurrent-shared-seat-activity\"",
        "pointerEvidence.quietObserved === true &&\n          pointerEvidence.concurrentSharedSeatActivityObserved === true",
        "rawCursorPositionsRetained: false",
        "rawPlatformActivityCountersRetained: false",
        "hidSystemActivityClaimedAsPhysical: false",
        "assertNoRetainedPointerRawData(serialized",
        "assertNoRetainedPointerRawData(persistedLog",
    ] {
        assert!(
            rig.contains(required),
            "missing v0.12.19 pointer contract: {required}"
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
    let prompt_sample_before = probe
        .find("private let pointerBeforePrompt = pointerSample()")
        .unwrap();
    let prompt_observation = probe
        .find("private let pointerPrompt = pointerPromptObservation(")
        .unwrap();
    let prompt_sample_after = probe
        .find(
            "private let pointer = pointerPrompt.requested ? pointerSample() : pointerBeforePrompt",
        )
        .unwrap();
    assert!(
        prompt_sample_before < prompt_observation && prompt_observation < prompt_sample_after,
        "prompt-bound HID counters must sandwich the exact prompt observation"
    );
    assert!(probe.contains(
        "let pointerActivityMonitorHealthy = pointerBeforePrompt.monitorHealthy && pointer.monitorHealthy"
    ));

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
fn macos_v0_12_14_quiet_lane_stabilizes_the_native_seat_before_candidate_execution() {
    let rig = fs::read_to_string("evidence/v0.12.19/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let probe = fs::read_to_string("evidence/v0.12.19/computer/SystemProbe.swift")
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
        "macOS quiet-seat execution regressions passed: stable completion, contamination reset, unknown refusal, immutable timeout.",
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
        .find("const stabilized = await runNativeQuietSeatStabilization({")
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
}

#[test]
fn macos_v0_12_14_pointer_handoff_is_passive_notification_only_and_fail_closed() {
    let prompt = fs::read_to_string("evidence/v0.12.19/computer/PointerHandoff.swift")
        .unwrap()
        .replace("\r\n", "\n");
    let probe = fs::read_to_string("evidence/v0.12.19/computer/SystemProbe.swift")
        .unwrap()
        .replace("\r\n", "\n");
    let rig = fs::read_to_string("evidence/v0.12.19/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let readme = fs::read_to_string("evidence/v0.12.19/computer/README.md")
        .unwrap()
        .replace("\r\n", "\n");
    let normalized_readme = readme.split_whitespace().collect::<Vec<_>>().join(" ");

    for required in [
        "private final class PassivePromptPanel: NSPanel",
        "override var canBecomeKey: Bool { false }",
        "override var canBecomeMain: Bool { false }",
        "styleMask: [.borderless, .nonactivatingPanel]",
        "panel.ignoresMouseEvents = true",
        "panel.acceptsMouseMovedEvents = false",
        "panel.hidesOnDeactivate = false",
        "application.setActivationPolicy(.accessory)",
        "case .waiting: \"LBB macOS Acceptance - WAITING\"",
        "case .move: \"LBB macOS Acceptance - MOVE POINTER\"",
        "case .action: \"LBB macOS Acceptance - ACTION RUNNING\"",
        "case .complete: \"LBB macOS Acceptance - COMPLETE\"",
        "func mayTransition(to next: PromptState) -> Bool",
        "case (.waiting, .move), (.move, .action), (.action, .move), (.action, .complete): true",
        "} else if next == .move {",
        "actionExpiresAt = nil",
        "private let armExpiresAt: Date",
        "private let hardExpiresAt: Date",
        "? armExpiresAt",
        ": actionExpiresAt ?? hardExpiresAt",
        "if remaining == 0",
        "hardRemaining <= 310",
        "completionGrace <= 10",
        "panel.setAccessibilityTitle(next.title)",
        "renameatx_np(",
        "UInt32(RENAME_EXCL)",
        "return fsync(directoryDescriptor) == 0",
        "arguments[0] == \"--publish-create-once\"",
        "guard arguments.count == 3",
        "precondition(!publishCreateOnce(",
        "macOS pointer handoff prompt self-test passed",
    ] {
        assert!(
            prompt.contains(required),
            "passive macOS pointer prompt is missing: {required}"
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
        "NSEvent.mouseEvent",
        "NSEvent.keyEvent",
        "mouseDown(",
        "mouseUp(",
    ] {
        assert!(
            !prompt.contains(forbidden),
            "notification-only prompt must not activate or synthesize input: {forbidden}"
        );
    }

    for required in [
        "private enum PointerPromptState: String",
        "case action = \"ACTION\"",
        "case .action: \"LBB macOS Acceptance - ACTION RUNNING\"",
        "private func pointerPromptObservation(",
        "for promptPID: pid_t",
        "expectedState: PointerPromptState?",
        "[.optionOnScreenOnly, .excludeDesktopElements]",
        "(window[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value == promptPID",
        "title == expectedState.title && alpha > 0 && width >= 1 && height >= 1",
        "exactWindows.count == 1 && ownedWindows.count == 1",
        "foregroundPID != promptPID",
        "frontmostAttribute(for: promptPID) == false",
        "\"pointerPromptOwnerMatched\": pointerPrompt.ownerMatched",
        "\"pointerPromptTitleMatched\": pointerPrompt.titleMatched",
        "\"pointerPromptOnScreen\": pointerPrompt.onScreen",
        "\"pointerPromptNonactivating\": pointerPrompt.nonactivating",
    ] {
        assert!(
            probe.contains(required),
            "SystemProbe omits exact prompt delivery/presentation proof: {required}"
        );
    }
    for raw_prompt_identity in ["\"pointerPromptPID\":", "\"pointerPromptTitle\":"] {
        assert!(
            !probe.contains(raw_prompt_identity),
            "SystemProbe must emit only bounded prompt-delivery booleans: {raw_prompt_identity}"
        );
    }

    for required in [
        "const POINTER_HANDOFF_REQUEST_FILE = \"macos-pointer-concurrency-handoff-request.json\"",
        "const POINTER_HANDOFF_COMPLETE_FILE = \"macos-pointer-concurrency-handoff-complete.json\"",
        "async function publishAtomicMarkerOnce(path, marker, timeoutMs = MARKER_PUBLISH_TIMEOUT_MS)",
        "open(temporaryPath, \"wx\", 0o600)",
        "await handle.sync()",
        "[\"--publish-create-once\", temporaryPath, path],",
        "kind: \"macos-pointer-concurrency-handoff-request\"",
        "kind: \"macos-pointer-concurrency-handoff-complete\"",
        "notificationOnly: true",
        "acceptedAsAuthority: false",
        "externalAcknowledgementConsumed: false",
        "markerAcceptedAsAuthority: false",
        "macOS packaged-evidence rig self-test passed.",
    ] {
        assert!(
            rig.contains(required),
            "one-shot notification marker contract is missing: {required}"
        );
    }
    let atomic_publication = rig
        .split(
            "async function publishAtomicMarkerOnce(path, marker, timeoutMs = MARKER_PUBLISH_TIMEOUT_MS)",
        )
        .nth(1)
        .unwrap()
        .split("async function writePointerHandoffState")
        .next()
        .unwrap();
    assert!(atomic_publication.contains("await handle.writeFile(serialized, \"utf8\")"));
    assert!(atomic_publication.contains("await handle.close()"));
    assert!(atomic_publication.contains("[\"--publish-create-once\", temporaryPath, path],"));
    assert!(atomic_publication.contains("timeoutMs,"));
    assert!(!atomic_publication.contains("rename("));

    for (marker_factory, exact_fields) in [
        (
            "function pointerHandoffRequestMarker(promptPid)",
            &[
                "schemaVersion:",
                "kind:",
                "productVersion:",
                "requestId:",
                "createdAt:",
                "runnerPid:",
                "promptPid,",
                "requestDelivered:",
                "panelOnScreen:",
                "panelNonactivating:",
                "notificationOnly:",
                "acceptedAsAuthority:",
            ][..],
        ),
        (
            "function pointerHandoffCompleteMarker(promptPid)",
            &[
                "schemaVersion:",
                "kind:",
                "productVersion:",
                "requestId:",
                "createdAt:",
                "runnerPid:",
                "promptPid,",
                "requestDelivered:",
                "panelOnScreen:",
                "panelNonactivating:",
                "notificationOnly:",
                "acceptedAsAuthority:",
                "sustainedMotionSamples:",
                "sustainedMotionSpanMilliseconds:",
                "productBoundaryContaminated:",
                "independentBoundaryContaminated:",
                "clickFreeMotionObserved:",
            ][..],
        ),
    ] {
        let marker = rig
            .split(marker_factory)
            .nth(1)
            .unwrap()
            .split("\n}\n")
            .next()
            .unwrap();
        let mut previous = 0;
        for field in exact_fields {
            let current = marker[previous..]
                .find(field)
                .map(|offset| previous + offset)
                .unwrap_or_else(|| panic!("{marker_factory} omits ordered field {field}"));
            assert!(current >= previous);
            previous = current + field.len();
        }
        assert!(marker.contains("notificationOnly: true"));
        assert!(marker.contains("acceptedAsAuthority: false"));
    }
    for forbidden in [
        "macos-pointer-concurrency-handoff-received.json",
        "externalAcknowledgementConsumed: true",
        "markerAcceptedAsAuthority: true",
    ] {
        assert!(
            !rig.contains(forbidden),
            "macOS handoff must never consume an external acknowledgement: {forbidden}"
        );
    }

    for required in [
        "const DELIBERATE_MOTION_REQUIRED_SAMPLES = 3",
        "const DELIBERATE_MOTION_MINIMUM_SPAN_MS = 500",
        "consecutive >= DELIBERATE_MOTION_REQUIRED_SAMPLES",
        "span >= DELIBERATE_MOTION_MINIMUM_SPAN_MS",
        "const progress = clickFreePointerMotionProgress(previous, sample)",
        "const moveInvariants = systemInvariants(previous, sample)",
        "const moveDisposition = preDispatchPointerTransitionDisposition(",
        "if (moveDisposition === \"unknown\")",
        "if (moveDisposition === \"rearm\")",
        "Pre-dispatch input or user-context activity reset the MOVE clean-motion arm; no product action was sent.",
        "function preDispatchPointerTransitionDisposition(progress, invariants)",
        "async function runPreDispatchPointerArmStateMachine({",
        "Pre-dispatch input or user-context activity reset the ACTION transition; no product action was sent.",
        "Pre-dispatch input or user-context activity reset the final ACTION boundary; no product action was sent.",
        "await transitionPrompt(POINTER_HANDOFF_MOVE_STATE)",
        "setActionDeadlineMilliseconds(null)",
        "const dispatchInvariants = systemInvariants(",
        "const dispatchDisposition = preDispatchPointerTransitionDisposition(",
        "if (dispatchDisposition === \"rearm\")",
        "const postResizeSystemBefore = pointerEvidenceLane === \"deliberate-concurrency\"\n    ? actionPromptBaseline\n    : processProbe(systemProbeBinary);",
        "function armProbeExpired(error, deadlineMilliseconds, nowMilliseconds = Date.now())",
        "error.code = \"SUBPROCESS_TIMEOUT\"",
        "if (armProbeExpired(error, armDeadlineMilliseconds, nowMilliseconds())) break",
        "macOS pointer-arm execution regressions passed: deadline expiry, MOVE re-arm, final ACTION re-arm.",
        "const clickFreeActionProgress = clickFreePointerMotionProgress(",
        "click, drag, scroll, or tablet activity invalidated the action boundary",
        "pointerHandoffClickFreeActionObserved = clickFreeActionProgress === \"advanced\"",
        "stage: \"waitDeliberatePointerActivity\"",
        "actionDispatched: false",
        "let pointerHandoffActionDispatched = false",
        "pointerHandoffActionDispatched = null",
        "postResizeClick.invariants?.inputDelivery?.dispatchAttemptRecorded === true",
        "failureProbeBaseline.actionDispatched = pointerHandoffActionDispatched",
        "pointerHandoffActionDispatched !== true",
        "const DELIBERATE_CONCURRENCY_ACTION_MS = 2_000",
        "const POINTER_HANDOFF_ACTION_STATE = \"ACTION\"",
        "pointerHandoffArmDeadlineMilliseconds",
        "pointerHandoffHardDeadlineMilliseconds",
        "pointerHandoffActionDeadlineMilliseconds",
        "const armProbeBudgetMs = armDeadlineMilliseconds - nowMilliseconds()",
        "Math.min(SYSTEM_PROBE_TIMEOUT_MS, armProbeBudgetMs)",
        "POINTER_HANDOFF_COMPLETION_RESERVE_MS",
        "AbortSignal.timeout(timeoutMs)",
        "timeout: timeoutMs",
        "remainingPointerHandoffTime(pointerHandoffActionDeadlineMilliseconds",
        "postResizeClickResponse.status === 400 && responseCode === \"BAD_REQUEST\"",
        "pointerLaneState(postResizeClick.invariants) === \"concurrent-shared-seat-activity\"",
        "pointerLaneState(postResizeActionInvariants) === \"concurrent-shared-seat-activity\"",
        "await completePointerHandoff(postResizeSystemAfter)",
        "await terminate(pointerHandoffProcess, \"pointer handoff prompt\")",
    ] {
        assert!(
            rig.contains(required),
            "macOS pointer arm/action/cleanup contract is missing: {required}"
        );
    }
    assert!(
        !rig.contains("click, drag, scroll, or tablet activity invalidated the pointer-motion arm")
    );
    assert!(
        !rig.contains("click, drag, scroll, or tablet activity invalidated the ACTION transition")
    );
    assert!(
        !rig.contains(
            "click, drag, scroll, or tablet activity invalidated the pre-dispatch boundary"
        )
    );

    let start_handoff = rig.find("await startPointerHandoff(").unwrap();
    let wait_stage = rig
        .find("stage: \"waitDeliberatePointerActivity\"")
        .unwrap();
    let pre_request_false = rig[wait_stage..]
        .find("actionDispatched: false")
        .map(|offset| wait_stage + offset)
        .unwrap();
    let sustained_arm = rig.find("await waitForDeliberatePointerActivity(").unwrap();
    let action_baseline = rig
        .find("const postResizeSystemBefore = pointerEvidenceLane === \"deliberate-concurrency\"")
        .unwrap();
    let dispatch_unknown = rig.find("pointerHandoffActionDispatched = null").unwrap();
    let action_command = rig
        .find("const postResizeClickResponse = await commandResponse(\n    \"computer.click\"")
        .unwrap();
    let action_result = rig
        .find("const postResizeClick = actionSummary(postResizeClickBody)")
        .unwrap();
    let evidence_bound_dispatch = rig
        .find("postResizeClick.invariants?.inputDelivery?.dispatchAttemptRecorded === true")
        .unwrap();
    let contamination_gate = rig
        .find("pointerHandoffProductBoundaryContaminated &&")
        .unwrap();
    let completion = rig
        .find("await completePointerHandoff(postResizeSystemAfter)")
        .unwrap();
    assert!(
        wait_stage < pre_request_false
            && pre_request_false < start_handoff
            && start_handoff < sustained_arm
            && sustained_arm < action_baseline
            && action_baseline < dispatch_unknown
            && dispatch_unknown < action_command
            && action_command < action_result
            && action_result < evidence_bound_dispatch
            && evidence_bound_dispatch < contamination_gate
            && contamination_gate < completion,
        "handoff dispatch state must be false before arm, unknown while awaiting, and true only from returned dispatch evidence"
    );

    let final_cleanup = rig.rsplit("} finally {").next().unwrap();
    let prompt_cleanup = final_cleanup
        .find("await terminate(pointerHandoffProcess, \"pointer handoff prompt\")")
        .unwrap();
    let helper_cleanup = final_cleanup
        .find("await terminate(helperProcess, \"helper\")")
        .unwrap();
    assert!(prompt_cleanup < helper_cleanup);

    let state_writer = rig
        .split("async function writePointerHandoffState(state)")
        .nth(1)
        .unwrap()
        .split("function pointerHandoffSummary()")
        .next()
        .unwrap();
    for allowed in [
        "POINTER_HANDOFF_MOVE_STATE",
        "POINTER_HANDOFF_ACTION_STATE",
        "POINTER_HANDOFF_COMPLETE_STATE",
    ] {
        assert!(
            state_writer.contains(allowed),
            "state writer omits {allowed}"
        );
    }
    assert!(!state_writer.contains("POINTER_HANDOFF_WAITING_STATE"));

    let prompt = fs::read_to_string("evidence/v0.12.19/computer/PointerHandoff.swift").unwrap();
    assert!(prompt.contains("if priorExpiration.timeIntervalSinceNow <= 0"));
    assert!(prompt.contains("guard transitionExpiration.timeIntervalSinceNow > 0 else"));

    let server = fs::read_to_string("src/server.rs").unwrap();
    let controller = fs::read_to_string("src/computer.rs").unwrap();
    assert!(rig.contains("const DELIBERATE_CONCURRENCY_ACTION_MS = 2_000"));
    assert!(server.contains("if !(50..=2_000).contains(&duration)"));
    assert!(controller.contains("const MAX_CURSOR_DURATION_MS: u64 = 2_000;"));

    for required in [
        "nonactivating",
        "atomically publishes create-once notifications",
        "three consecutive",
        "500 ms",
        "two-second",
        "ACTION RUNNING",
        "clickFreeMotionObserved",
        "green completion state",
        "10-second",
        "never reads an external acknowledgement",
        "actionDispatched",
        "waitDeliberatePointerActivity",
    ] {
        assert!(
            normalized_readme.contains(required),
            "macOS pointer handoff boundary is undocumented: {required}"
        );
    }
}

#[test]
fn macos_pointer_arm_state_machine_execution_regressions_pass() {
    let output = match Command::new("node")
        .args([
            "evidence/v0.12.19/computer/helper-evidence-rig.mjs",
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
    assert!(stdout.contains("macOS packaged-evidence rig self-test passed."));
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
fn macos_candidate_evidence_targets_current_version_and_only_reduced_outputs() {
    let entries = fs::read_dir("evidence/v0.12.19/computer")
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
            "HelperEvidenceFixture.swift".to_owned(),
            "PointerHandoff.swift".to_owned(),
            "README.md".to_owned(),
            "SystemProbe.swift".to_owned(),
            "helper-evidence-rig.mjs".to_owned(),
        ])
    );

    let rig = fs::read_to_string("evidence/v0.12.19/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let fixture =
        fs::read_to_string("evidence/v0.12.19/computer/HelperEvidenceFixture.swift").unwrap();
    let readme = fs::read_to_string("evidence/v0.12.19/computer/README.md").unwrap();

    assert!(rig.contains(&format!("const EXPECTED_VERSION = \"{VERSION}\";")));
    assert!(rig.contains("const EXPECTED_ARCHIVE = `local-browser-bridge-v${EXPECTED_VERSION}-macos-universal.tar.gz`;"));
    assert!(rig.contains("status: \"passed-release-candidate\""));
    assert!(rig.contains("evidenceClass: \"exact-release-candidate-package-live-observation\""));
    assert!(rig.contains("candidateNotice:"));
    assert!(fixture.contains(&format!("LBB v{VERSION} Persistent SCStream Evidence")));
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
    assert!(readme.contains(&format!("macOS v{VERSION} server and helper")));
    assert!(readme.contains(&format!(
        "local-browser-bridge-v{VERSION}-macos-universal.tar.gz"
    )));
    assert!(!rig.replace("v0.12.19", "").contains("v0.12.1"));
    assert!(!fixture.replace("v0.12.19", "").contains("v0.12.1"));
    assert!(!rig.contains("v0.12.2"));
    assert!(!fixture.contains("v0.12.2"));
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
    let rig = fs::read_to_string("evidence/v0.12.19/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let readme = fs::read_to_string("evidence/v0.12.19/computer/README.md")
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
        "TAG_OBJECT_SHA",
        "release-candidate",
        "raw artifact ZIP size mismatch",
        "outer artifact ZIP inventory changed before bounded extraction",
        "checksum manifest line count mismatch",
        "candidate payload checksum mismatch",
        "gh attestation verify",
        "--source-ref \"refs/tags/$TAG\"",
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
    let rig = fs::read_to_string("evidence/v0.12.19/computer/helper-evidence-rig.mjs")
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
        "ten PAX-free regular-file/directory entries passed bounded streaming extraction",
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
        10,
        "macOS package inventory must contain exactly ten entries"
    );
    for required_path in [
        "local-browser-bridge",
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
    let rig = repository.join("evidence/v0.12.19/computer/helper-evidence-rig.mjs");
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
helper = b'#!/bin/sh\nprintf executed > "$(dirname "$0")/HELPER_EXECUTED"\nexit 91\n'
entries = [
    ["local-browser-bridge", "file", 0o755, server],
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
        let archive = case_root.join("local-browser-bridge-v0.12.19-macos-universal.tar.gz");
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
            "{zero_hash}  local-browser-bridge-v0.12.19-windows-x86_64.exe\n\
             {zero_hash}  local-computer-helper-v0.12.19-windows-x86_64.exe\n\
             {archive_sha256}  local-browser-bridge-v0.12.19-macos-universal.tar.gz\n\
             {zero_hash}  local-browser-bridge-extension-v0.12.19.zip\n"
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
                .join("Local Computer Helper.app/Contents/MacOS/HELPER_EXECUTED")
                .exists()
        );
        let mut inventory = BTreeSet::new();
        collect_inventory(&output_dir, &output_dir, &mut inventory);
        assert_eq!(
            inventory,
            BTreeSet::from([
                "LICENSE".to_string(),
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
fn macos_packaged_evidence_uses_a_clean_tagged_harness_and_fresh_lane_outputs() {
    let rig = fs::read_to_string("evidence/v0.12.19/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "expectedSourceSha",
        "expectedTagObjectSha",
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
    let rig = fs::read_to_string("evidence/v0.12.19/computer/helper-evidence-rig.mjs").unwrap();
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
    let rig = fs::read_to_string("evidence/v0.12.19/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    assert!(rig.contains("function childEnvironment(overrides = {})"));
    assert!(!rig.contains("...process.env"));
    let fixture = fs::read_to_string("evidence/v0.12.19/computer/HelperEvidenceFixture.swift")
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
        "override func mouseMoved(with event: NSEvent)",
        "state.moveEvents < 1_000_000",
        "window.acceptsMouseMovedEvents = true",
    ] {
        assert!(
            fixture.contains(required),
            "missing fixture focus proof: {required}"
        );
    }
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
    assert!(rig.contains("schemaVersion: 6"));
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
    let rig = fs::read_to_string("evidence/v0.12.19/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let readme = fs::read_to_string("evidence/v0.12.19/computer/README.md")
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
fn macos_packaged_evidence_proves_same_pid_sibling_routing_without_unsafe_negative() {
    let fixture = fs::read_to_string("evidence/v0.12.19/computer/HelperEvidenceFixture.swift")
        .unwrap()
        .replace("\r\n", "\n");
    let probe = fs::read_to_string("evidence/v0.12.19/computer/SystemProbe.swift")
        .unwrap()
        .replace("\r\n", "\n");
    let rig = fs::read_to_string("evidence/v0.12.19/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let readme = fs::read_to_string("evidence/v0.12.19/computer/README.md")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "private let siblingFixtureTitle = \"LBB v0.12.19 Same-PID Sibling Receiver\"",
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
    assert_eq!(fixture.matches("= NSWindow(").count(), 2);
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
        "local-browser-bridge $Version",
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
        "[int64]$children[0] -eq $reportedWorkerPid",
        "$stablePolls -ge 2",
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
    assert!(server.contains("&& process_id.is_some()"));
    assert!(server.contains("process_id: process_id.unwrap_or(0)"));
    assert!(ci.contains("scripts/test-windows-computer-use.ps1"));
    assert!(deploy.contains("scripts/test-windows-computer-use.ps1"));
    for workflow in [&ci, &deploy] {
        assert!(workflow.contains(
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/test-windows-computer-use.ps1 -SelfTest"
        ));
        assert!(workflow.contains("& ./scripts/test-windows-computer-use.ps1 -SelfTest"));
        assert!(workflow.contains(
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./tests/fixtures/windows/WindowsComputerUseFixture.ps1 -SelfTest"
        ));
        assert!(
            !workflow
                .contains("& ./tests/fixtures/windows/WindowsComputerUseFixture.ps1 -SelfTest"),
            "the .NET Framework fixture must be parsed by PowerShell Core but executed by Windows PowerShell 5.1"
        );
    }
    for required in [
        "$selfTestJob.StartProcess(",
        "GetDirectChildProcessIds($PID, $selfTestHostPath)",
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
        "Wait-ForDirectHelperWorker $helperProcess $initialHelperSessionId $postArmHelperDescription",
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
        "Text = \"LBB Windows Acceptance - ACTION REQUIRED\";",
        "statusLabel.Text = \"ACTION REQUIRED\\r\\nClick once, then stop using this session\";",
        "Text = \"LBB Windows Acceptance - ARMED\";",
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
fn windows_foreground_arm_handoff_watcher_is_strict_read_only_and_non_authoritative() {
    let watcher = fs::read_to_string("scripts/wait-windows-foreground-arm-handoff.ps1")
        .unwrap()
        .replace("\r\n", "\n");
    let runner = fs::read_to_string("scripts/test-windows-computer-use.ps1")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "$script:ProductVersion = \"0.12.19\"",
        "$script:MarkerSchemaVersion = 2",
        "function Assert-ExactPropertyOrder {",
        "function Assert-ExactMarkerSchema {",
        "function Resolve-OrdinaryEvidenceDirectory {",
        "function Read-AtomicRequestMarker {",
        "function Assert-BoundRunnerState {",
        "function Assert-FreshMarkerBinding {",
        "function New-SanitizedHandoff {",
        "function Wait-ForegroundArmHandoff {",
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
    assert!(runner.contains("-ProductVersion $Version"));
    assert!(runner.contains("-ProductVersion \"0.12.19\""));
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
        .split("fn post_mouse(")
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

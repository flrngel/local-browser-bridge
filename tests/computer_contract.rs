use std::collections::BTreeSet;
use std::fs;
use std::process::Command;

use local_browser_bridge::VERSION;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use local_browser_bridge::computer::ComputerController;
use local_browser_bridge::computer::{
    COMPUTER_METHODS, COMPUTER_TYPE_TEXT_MAX_DISPATCH_MS, COMPUTER_TYPE_TEXT_MAX_UTF16_UNITS,
};

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

#[cfg(any(target_os = "macos", target_os = "windows"))]
#[test]
fn helper_machine_contract_discloses_platform_activation_truthfully() {
    let hello = ComputerController::new().hello();
    let invariants = &hello["invariants"];

    assert_eq!(invariants["globalHidInput"], false);
    assert_eq!(invariants["movesHardwareCursor"], false);
    assert_eq!(invariants["foregroundIdentityPreservedBeforeAfter"], true);
    assert_eq!(invariants["hardwareCursorPreservedBeforeAfter"], true);
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
        "SendInput(",
        "SetForegroundWindow(",
        "keybd_event(",
    ] {
        assert!(
            !macos.contains(forbidden) && !windows.contains(forbidden),
            "found forbidden foreground fallback: {forbidden}"
        );
    }
    assert!(macos.contains("SLEventPostToPid"));
    assert!(macos.contains("CGEventSetWindowLocation"));
    assert!(windows.contains("PostMessageW"));
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
        .split("fn guarded_semantic<T>(")
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
    let macos = fs::read_to_string("src/computer/platform_macos.rs").unwrap();
    let windows = fs::read_to_string("src/computer/platform_windows.rs").unwrap();
    let source = format!("{controller}\n{macos}\n{windows}");

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
            controller.contains(stage),
            "missing invariant stage {stage}"
        );
    }
    for invariant in [
        "foregroundUnchanged",
        "userFocusUnchanged",
        "hardwareCursorUnchanged",
        "desktopSpaceUnchanged",
    ] {
        assert!(
            controller.contains(invariant),
            "missing invariant name {invariant}"
        );
    }

    assert!(controller.contains("stage={};failedInvariants={}"));
    assert!(!source.contains(".assert_held()"));
    assert!(source.contains("InvariantStage::PointerTrajectory"));
    assert!(source.contains("InvariantStage::ClickDispatch"));
    assert!(source.contains("InvariantStage::DragDispatch"));
    assert!(source.contains("InvariantStage::ScrollDispatch"));
    assert!(!controller.contains(
        "The foreground, hardware cursor, or active desktop changed during background delivery"
    ));
}

#[test]
fn macos_negative_evidence_keeps_only_equality_and_fixture_counters() {
    let rig = fs::read_to_string("evidence/v0.12.1/computer/helper-evidence-rig.mjs").unwrap();
    assert!(rig.contains("failureProbeBaseline"));
    assert!(rig.contains("collectFailureDiagnostics"));
    assert!(rig.contains("systemInvariants(failureProbeBaseline.system, after)"));
    assert!(rig.contains("fixtureCounterSnapshot"));
    assert!(rig.contains("semanticValueMatchesExpected"));
    assert!(rig.contains("failureDiagnostics,"));

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
fn macos_packaged_evidence_is_bound_to_an_out_of_band_canonical_manifest() {
    let rig = fs::read_to_string("evidence/v0.12.1/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let readme = fs::read_to_string("evidence/v0.12.1/computer/README.md")
        .unwrap()
        .replace("\r\n", "\n");

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
        "manifestSha256 === expectedManifestSha256",
        "checksum manifest has the exact canonical four-entry set",
        "archive checksum is bound by the canonical manifest",
        "checksumManifest: manifestBinding",
        "packageBinding: manifestBinding",
    ] {
        assert!(
            rig.contains(required),
            "macOS evidence manifest binding is missing: {required}"
        );
    }
    for required in [
        "$EXPECTED_SHA256SUMS_SHA256",
        "mandatory SHA-256 supplied",
        "exactly the four v0.12.1",
        "expected and actual manifest hashes",
    ] {
        assert!(
            readme.contains(required),
            "macOS evidence manifest handoff is undocumented: {required}"
        );
    }
}

#[test]
fn macos_resize_evidence_requires_a_settled_geometry_bound_frame() {
    let rig = fs::read_to_string("evidence/v0.12.1/computer/helper-evidence-rig.mjs").unwrap();
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
    let rig = fs::read_to_string("evidence/v0.12.1/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    assert!(rig.contains("function childEnvironment(overrides = {})"));
    assert!(!rig.contains("...process.env"));
    let fixture = fs::read_to_string("evidence/v0.12.1/computer/HelperEvidenceFixture.swift")
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
        "command(\"computer.click\"",
        "post-resize pixel click target-side proof",
        "postResizeFixtureAfter.clicks === 2",
        "postResizeFixtureAfter.semanticPresses === postResizeFixtureBefore.semanticPresses",
        "postResizeFixtureAfter.resizeCount === postResizeFixtureBefore.resizeCount",
        "postResizeClick.frameId === observation.frameId",
        "postResizeClick.shareId === firstShareId",
        "postResizeClick.sourceSequence === beforePostResizeAction.sourceSequence",
        "post-resize independent foreground/focus/cursor/Space invariants",
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
        "native typeText independent foreground/focus/cursor/Space invariants",
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
        "cancellation/stop foreground/focus/cursor/Space invariants",
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
    assert!(rig.contains("schemaVersion: 3"));
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
    let rig = fs::read_to_string("evidence/v0.12.1/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let readme = fs::read_to_string("evidence/v0.12.1/computer/README.md")
        .unwrap()
        .replace("\r\n", "\n");

    let share_start = rig.find("const targetCloseShareStartBody").unwrap();
    let active_frame = rig.find("const targetCloseActive").unwrap();
    let target_termination = rig.find("const fixtureTargetClosure").unwrap();
    let terminal_state = rig.find("const targetClosedState").unwrap();
    let stale_refusal = rig.find("const targetClosedStaleAction").unwrap();
    let closed_observe = rig.find("const targetClosedObserve").unwrap();
    let target_absent = rig.find("const targetClosedStatusBody").unwrap();
    let helper_teardown = rig.find("const helperTeardown").unwrap();
    assert!(
        share_start < active_frame
            && active_frame < target_termination
            && target_termination < terminal_state
            && terminal_state < stale_refusal
            && stale_refusal < closed_observe
            && closed_observe < target_absent
            && target_absent < helper_teardown,
        "exact-target close evidence must follow the live share boundary and precede generic teardown"
    );

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
        "target-close foreground/focus/cursor/Space invariants",
        "fixtureProcess = null",
    ] {
        assert!(
            target_close.contains(required),
            "missing exact-target close proof: {required}"
        );
    }
    assert!(rig.contains("\"COMPUTER_NO_WINDOW\", \"COMPUTER_CAPTURE_FAILED\""));
    assert!(rig.contains("exactTargetClose: {"));
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
        "While that share is active, the runner sends `SIGTERM` only",
        "The server must mark the share stopped with",
        "`NO_COMPUTER_FRAME`",
        "`COMPUTER_NO_WINDOW`",
        "never closes an unrelated application or window",
        "only after it has requested and observed exit of its exact spawned fixture",
    ] {
        assert!(
            readme.contains(required),
            "evidence README is missing target-close contract: {required}"
        );
    }
}

#[test]
fn macos_packaged_evidence_proves_same_pid_sibling_routing_without_unsafe_negative() {
    let fixture = fs::read_to_string("evidence/v0.12.1/computer/HelperEvidenceFixture.swift")
        .unwrap()
        .replace("\r\n", "\n");
    let probe = fs::read_to_string("evidence/v0.12.1/computer/SystemProbe.swift")
        .unwrap()
        .replace("\r\n", "\n");
    let rig = fs::read_to_string("evidence/v0.12.1/computer/helper-evidence-rig.mjs")
        .unwrap()
        .replace("\r\n", "\n");
    let readme = fs::read_to_string("evidence/v0.12.1/computer/README.md")
        .unwrap()
        .replace("\r\n", "\n");

    for required in [
        "private let siblingFixtureTitle = \"LBB v0.12.1 Same-PID Sibling Receiver\"",
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
        .split("function allInvariantsHeld")
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
        "same-PID pixel routing preserves user foreground/focus/cursor/Space",
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
    ] {
        assert!(
            runner.contains(required),
            "Windows candidate binding is missing {required}"
        );
    }
    assert!(development.contains("out-of-band binding value"));
    assert!(development.contains("candidateBinding.checksumManifestMatched: true"));

    let candidate_binding = runner.find("$candidateBinding = [ordered]@{").unwrap();
    let evidence_creation = runner
        .find("[IO.Directory]::CreateDirectory($evidenceRoot)")
        .unwrap();
    let first_process_launch = runner.find("Start-IsolatedProcess $hostPath").unwrap();
    assert!(candidate_binding < evidence_creation);
    assert!(candidate_binding < first_process_launch);
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
        assert!(platform.contains("Ok((backend_effect, report))"));
    }

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

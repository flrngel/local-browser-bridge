use std::collections::BTreeSet;
use std::fs;
use std::process::Command;

use local_browser_bridge::VERSION;
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
    assert!(windows.contains("self.input_desktop == after.input_desktop"));
    assert!(!windows.contains("space_unchanged: true"));
    assert!(windows.contains("GetForegroundWindow returned no readable window"));
    assert!(windows.contains("GetGUIThreadInfo failed for the foreground thread"));
    assert!(macos.contains("resolved_front_identity"));
    assert!(macos.contains(
        "struct DesktopSnapshot {\n    front_process: [u8; 8],\n    front_pid: u32,\n    front_window_id: u32,"
    ));
    assert!(macos.contains("&lease.previous_psn"));
    assert!(macos.contains("lease.previous_window_id"));
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
    assert!(macos.contains("TEXT_TARGET_REVALIDATE_SCALARS"));
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
    ] {
        assert!(
            !diagnostics.contains(raw_identity),
            "failure evidence persisted raw identity {raw_identity}"
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
        "func focusSemanticField(controlSequence: Int) -> Bool",
        "window.makeFirstResponder(semanticField)",
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
        .split("const systemAfter = processProbe(systemProbeBinary);")
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

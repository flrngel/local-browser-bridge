use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

use local_browser_bridge::VERSION;
use local_browser_bridge::server::ACTION_METHODS;
use serde_json::Value;

const EXTENSION_FILES: &[&str] = &[
    "background.js",
    "content.js",
    "dom-core.js",
    "frame-agent.js",
    "lib.js",
    "manifest.json",
    "popup.css",
    "popup.html",
    "popup.js",
];

fn normalize_newlines(source: String) -> String {
    source.replace("\r\n", "\n")
}

#[test]
fn node_runtime_is_available_for_extension_behavior_contracts() {
    let output = Command::new("node")
        .arg("--version")
        .output()
        .expect("Node.js is a test-only dependency for extension behavior contracts; it is not an end-user runtime dependency");
    assert!(
        output.status.success(),
        "Node.js could not execute the extension behavior contracts: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn extension_source(file: impl AsRef<Path>) -> String {
    normalize_newlines(fs::read_to_string(Path::new("extension").join(file)).unwrap())
}

fn manifest() -> Value {
    serde_json::from_str(&extension_source("manifest.json")).unwrap()
}

fn strings(value: &Value) -> BTreeSet<String> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item.as_str().unwrap().to_owned())
        .collect()
}

fn quoted_strings(source: &str) -> BTreeSet<String> {
    let mut strings = BTreeSet::new();
    let mut quote = None;
    let mut current = String::new();
    let mut escaped = false;
    for character in source.chars() {
        if let Some(delimiter) = quote {
            if escaped {
                current.push(character);
                escaped = false;
            } else if character == '\\' {
                escaped = true;
            } else if character == delimiter {
                strings.insert(std::mem::take(&mut current));
                quote = None;
            } else {
                current.push(character);
            }
        } else if character == '"' || character == '\'' {
            quote = Some(character);
        }
    }
    strings
}

#[test]
fn source_contract_reader_normalizes_windows_line_endings() {
    let lf = "async function example() {\n  return true;\n}\n";
    let crlf = lf.replace('\n', "\r\n");
    assert_eq!(normalize_newlines(lf.to_owned()), lf);
    assert_eq!(normalize_newlines(crlf), lf);
}

#[test]
fn manifest_has_only_reviewed_capabilities() {
    let manifest = manifest();
    assert_eq!(manifest["manifest_version"], 3);
    assert_eq!(manifest["version"], VERSION);
    assert_eq!(manifest["minimum_chrome_version"], "140");
    assert_eq!(
        strings(&manifest["permissions"]),
        BTreeSet::from_iter(
            [
                "alarms",
                "debugger",
                "scripting",
                "storage",
                "tabGroups",
                "tabs",
            ]
            .map(str::to_owned)
        )
    );
    assert_eq!(
        strings(&manifest["host_permissions"]),
        BTreeSet::from_iter(["file://*/*", "http://*/*", "https://*/*"].map(str::to_owned))
    );
    for forbidden in [
        "cookies",
        "downloads",
        "management",
        "nativeMessaging",
        "proxy",
        "webRequest",
    ] {
        assert!(!strings(&manifest["permissions"]).contains(forbidden));
    }
    assert!(manifest.get("externally_connectable").is_none());
    assert_eq!(
        manifest["content_security_policy"]["extension_pages"],
        "script-src 'self'; object-src 'none'"
    );
}

#[test]
fn package_contains_only_declared_local_assets() {
    let actual = fs::read_dir("extension")
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual,
        EXTENSION_FILES
            .iter()
            .map(|file| (*file).to_owned())
            .collect()
    );

    let manifest = manifest();
    for path in [
        manifest["background"]["service_worker"].as_str().unwrap(),
        manifest["action"]["default_popup"].as_str().unwrap(),
        manifest["content_scripts"][0]["js"][0].as_str().unwrap(),
    ] {
        assert!(
            Path::new("extension").join(path).is_file(),
            "missing {path}"
        );
    }
    let popup = extension_source("popup.html");
    assert!(popup.contains("href=\"popup.css\""));
    assert!(popup.contains("src=\"popup.js\""));

    // Both release scripts hardcode the packaged file list. Pinning them to
    // the same set here is what stops a new extension file from shipping in a
    // zip that silently omits it.
    let declared = format!("{} LICENSE", EXTENSION_FILES.join(" "));
    let package = fs::read_to_string("scripts/package-extension.sh").unwrap();
    let verify = fs::read_to_string("scripts/verify-release-assets.sh").unwrap();
    assert!(
        package.contains(&format!("files=({declared})")),
        "scripts/package-extension.sh does not package exactly: {declared}"
    );
    assert!(
        verify.contains(&format!("expected_extension_files=({declared})")),
        "scripts/verify-release-assets.sh does not verify exactly: {declared}"
    );

    // The content script loads the shared DOM core first; the frame agent is
    // never a content script and never web accessible.
    assert_eq!(
        strings(&manifest["content_scripts"][0]["js"]),
        BTreeSet::from_iter(["content.js", "dom-core.js"].map(str::to_owned))
    );
    assert_eq!(manifest["content_scripts"][0]["js"][0], "dom-core.js");
    assert_eq!(manifest["content_scripts"][0]["js"][1], "content.js");
    assert!(manifest.get("web_accessible_resources").is_none());
}

#[test]
fn extension_executes_no_remote_code_or_update_client() {
    let source = EXTENSION_FILES
        .iter()
        .filter(|file| file.ends_with(".js") || file.ends_with(".html"))
        .map(extension_source)
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "fetch(",
        "XMLHttpRequest",
        "EventSource(",
        "import(\"http",
        "from \"http",
        "github.com",
        "api.github.com",
    ] {
        assert!(
            !source.contains(forbidden),
            "found forbidden source: {forbidden}"
        );
    }
    let background = extension_source("background.js");
    assert!(background.contains("new WebSocket(`ws://127.0.0.1:"));
    assert!(!background.contains("wss://"));
}

#[test]
fn extension_allows_only_the_demo_on_the_bridge_origin() {
    let library = extension_source("lib.js");
    assert!(library.contains("bridgeOrigin && url.pathname !== \"/demo\""));
    assert!(library.contains("The bridge cannot control its own control surface"));
    assert!(!library.contains("url.pathname.startsWith(\"/api\")"));
}

#[test]
fn observations_are_non_activating_bounded_and_composed() {
    let background = extension_source("background.js");
    let content = extension_source("content.js");
    let core = extension_source("dom-core.js");
    let capture_start = background.find("async function captureTab").unwrap();
    let capture_end = background[capture_start..]
        .find("async function debuggerCommand")
        .unwrap()
        + capture_start;
    let capture = &background[capture_start..capture_end];
    assert!(capture.contains("Page.captureScreenshot"));
    assert!(capture.contains("control.capture.begin"));
    assert!(capture.contains("control.capture.end"));
    assert!(capture.contains("finally"));
    assert!(!capture.contains("chrome.tabs.update"));
    assert!(background.contains("DEBUGGER_TIMEOUT"));
    assert!(background.contains("finally"));
    assert!(content.contains("composedCandidates"));
    assert!(core.contains("element.shadowRoot"));
    assert!(core.contains("tree: element.getRootNode() instanceof ShadowRoot"));
}

#[test]
fn direct_input_requires_a_fresh_snapshot() {
    let background = extension_source("background.js");
    let content = extension_source("content.js");
    for method in ["page.key", "page.clickAt", "page.typeText"] {
        let start = background.find(&format!("case \"{method}\"")).unwrap();
        let end = background[start + 5..]
            .find("\n    case \"")
            .map(|end| start + 5 + end)
            .unwrap_or(background.len());
        let body = &background[start..end];
        let snapshot_guard = if method == "page.clickAt" {
            body.contains("preparePoint") && body.contains("generation: params.generation")
        } else {
            body.contains("assertGeneration")
        };
        assert!(snapshot_guard, "{method} is not snapshot-bound");
    }
    assert!(content.contains("STALE_SNAPSHOT: observe the page again before acting"));
}

#[test]
fn debugger_control_is_persistent_bounded_and_revocable() {
    let background = extension_source("background.js");
    let content = extension_source("content.js");
    for method in [
        "browser.control.start",
        "browser.control.status",
        "browser.control.stop",
    ] {
        assert!(background.contains(method), "missing {method}");
    }
    assert!(background.contains("CONTROL_TTL_MIN_MS"));
    assert!(background.contains("CONTROL_TTL_MAX_MS"));
    assert!(background.contains("CONTROL_HEARTBEAT_MS"));
    assert!(background.contains("chrome.storage.session"));
    assert!(background.contains("chrome.debugger.onDetach.addListener"));
    assert!(background.contains("requiresExplicitStart: true"));
    assert!(background.contains("CONTROL_REVOKED"));

    // Debugger attachment belongs to the lease, not to an individual command.
    assert_eq!(background.matches("chrome.debugger.attach(").count(), 1);
    assert_eq!(background.matches("chrome.debugger.detach(").count(), 1);
    assert!(background.contains("async function boundedDebuggerDetach"));
    assert!(!background.contains("withDebugger"));
    assert!(!background.contains("clickFallback"));
    assert!(!background.contains("element.click()"));
    assert!(content.contains("revokedControlSessions"));
    assert!(content.contains("assertActiveControl"));
    assert!(!content.contains("element.click()"));
    let stop_start = background.find("async function stopControl").unwrap();
    let stop_end = background[stop_start..]
        .find("async function hardRevokeDetached")
        .unwrap()
        + stop_start;
    let stop = &background[stop_start..stop_end];
    assert!(stop.contains("synchronouslyTakeControlLease"));
    assert!(stop.find("releaseHeldInputs").unwrap() < stop.find("boundedDebuggerDetach").unwrap());
}

#[test]
fn transport_and_security_rotation_revoke_control_first() {
    let background = extension_source("background.js");
    let clear_start = background.find("async function clearSocket").unwrap();
    let clear_end = background[clear_start..]
        .find("function settingFingerprint")
        .unwrap()
        + clear_start;
    let clear = &background[clear_start..clear_end];
    assert!(clear.contains("cancelCommandContextsForSession(protocolServerSessionId, reason)"));
    assert!(clear.contains("await stopControl(reason, { requireExplicitStart: true })"));
    assert!(
        clear
            .find("await stopControl(reason, { requireExplicitStart: true })")
            .unwrap()
            < clear.find("socket.close()").unwrap()
    );

    let update_start = background.find("function updateSecuritySettings").unwrap();
    let update_end = background[update_start..]
        .find("function queueTransportRotation")
        .unwrap()
        + update_start;
    let update = &background[update_start..update_end];
    assert!(
        update.find("await clearSocket(reason)").unwrap()
            < update.find("chrome.storage.local.set").unwrap()
    );
    for action in ["saveConnection", "toggleEnabled", "toggleFullAccess"] {
        let start = background.find(&format!("case \"{action}\"")).unwrap();
        let end = background[start + 5..]
            .find("\n      case \"")
            .map(|offset| start + 5 + offset)
            .unwrap_or(background.len());
        assert!(background[start..end].contains("updateSecuritySettings"));
    }
    let save_start = background.find("case \"saveConnection\"").unwrap();
    let save_end = background[save_start + 5..]
        .find("\n      case \"")
        .map(|offset| save_start + 5 + offset)
        .unwrap_or(background.len());
    let save = &background[save_start..save_end];
    assert!(save.contains("decodeBase64Url32(token, \"Extension token\")"));
    assert!(save.find("decodeBase64Url32").unwrap() < save.find("updates.token = token").unwrap());
    assert!(background.contains("external_security_settings_changed"));
    assert!(background.contains("consumeInternalSettings(changes)"));
}

#[test]
fn extension_storage_is_restricted_to_trusted_contexts_before_use() {
    let background = extension_source("background.js");
    let access = background
        .find("chrome.storage.local.setAccessLevel")
        .expect("storage access boundary");
    let first_read = background
        .find("chrome.storage.local.get")
        .expect("storage read");
    let first_write = background
        .find("chrome.storage.local.set({")
        .expect("storage write");

    assert!(background.contains("accessLevel: \"TRUSTED_CONTEXTS\""));
    assert!(access < first_read);
    assert!(access < first_write);
    assert!(background.contains("async function settings() {\n  await trustedStorageReady;"));
    assert!(background.contains(
        "async function setStatus(status, detail = \"\") {\n  await trustedStorageReady;"
    ));
    assert!(background.contains(
        "controlStatePromise = (async () => {\n    await trustedStorageReady;\n    const [stored, storedPause]"
    ));
    assert!(
        background
            .contains("void trustedStorageReady.then(() => chrome.storage.local.get(DEFAULTS))")
    );
}

#[test]
fn control_lease_is_owned_by_the_authenticated_server_session() {
    let background = extension_source("background.js");
    assert!(background.contains("currentControlOwner"));
    assert!(background.contains("protocolSessionReady && protocolServerSessionId"));
    assert!(background.contains("`server:${protocolServerSessionId}`"));
    assert!(background.contains("ownerSessionId: requestedOwner"));
    assert!(background.contains("controlLease.ownerSessionId !== requestedOwner"));
    assert!(background.contains("CONTROL_OWNER_MISMATCH"));
    assert!(background.contains("lease_owner_missing"));
    assert!(background.contains("owner_session_changed"));
}

#[test]
fn trusted_pointer_has_dynamic_motion_and_two_phase_target_validation() {
    let background = extension_source("background.js");
    let content = extension_source("content.js");
    let core = extension_source("dom-core.js");

    assert!(background.contains("boundedBezierSpringPath"));
    assert!(background.contains("POINTER_CANDIDATE_COUNT = 20"));
    assert!(background.contains("scorePointerCandidate"));
    assert!(background.contains("boundaryPenalty"));
    assert!(background.contains("curvaturePenalty"));
    assert!(background.contains("reversePenalty"));
    assert!(background.contains("minimumJerk"));
    assert!(background.contains("springRate"));
    assert!(background.contains("moveSequence"));
    assert!(background.contains("turn"));
    assert!(background.contains("control.cursor"));
    assert!(background.contains("Input.dispatchMouseEvent"));
    assert!(background.contains("method: \"commitClick\""));
    assert!(background.contains("method: \"commitPoint\""));

    assert!(content.contains("prepareClick"));
    assert!(content.contains("commitClick"));
    assert!(content.contains("preparePoint"));
    assert!(content.contains("commitPoint"));
    assert!(content.contains("targetSignature"));
    assert!(core.contains("sameBounds"));
    assert!(core.contains("deepElementFromPoint"));
    assert!(core.contains("TARGET_OCCLUDED"));
    assert!(core.contains("TARGET_CHANGED"));
}

#[test]
fn background_pointer_teleports_once_without_focus_or_animation_latency() {
    let background = extension_source("background.js");
    let presentation_start = background
        .find("async function pointerPresentationState")
        .unwrap();
    let presentation_end = background[presentation_start..]
        .find("function assertPointerArrival")
        .unwrap()
        + presentation_start;
    let presentation = &background[presentation_start..presentation_end];
    assert!(presentation.contains("chrome.tabs.get(tabId)"));
    assert!(presentation.contains("target tab presentation confirmation"));
    assert!(presentation.contains("confirmedTab?.windowId !== tab.windowId"));
    assert!(presentation.contains("reason: \"tab_presentation_changed\""));
    assert!(presentation.contains("chrome.windows.get(tab.windowId)"));
    assert!(presentation.contains("confirmedTab.active === true"));
    assert!(presentation.contains("confirmedWindow.focused === true"));
    assert!(presentation.contains("target window presentation confirmation"));
    assert!(presentation.contains("reason: \"window_presentation_changed\""));
    assert!(presentation.contains("windowState === \"normal\""));
    assert!(!presentation.contains("chrome.tabs.update"));
    assert!(!presentation.contains("chrome.windows.update"));

    let movement_start = background.find("async function moveVirtualCursor").unwrap();
    let movement_end = background[movement_start..]
        .find("async function trustedClick")
        .unwrap()
        + movement_start;
    let movement = &background[movement_start..movement_end];
    assert!(movement.contains("if (!presentation.animate)"));
    assert!(movement.contains("await dispatchPointerPoint(target)"));
    assert!(
        movement
            .matches("pointerPresentationState(tabId, authority, commandContext)")
            .count()
            >= 3
    );
    assert!(movement.contains("arrival: skippedAnimation ? \"skipped_background\" : \"arrived\""));
    assert!(movement.contains("profile: skippedAnimation ? \"background-final-arrival\" : \"bounded-cubic-minimum-jerk-spring\""));

    for function in [
        "async function trustedClick(",
        "async function trustedClickAt(",
    ] {
        let start = background.find(function).unwrap();
        let end = background[start..].find("\n}\n").unwrap() + start + 3;
        let click = &background[start..end];
        assert!(
            click.find("assertPointerArrival(motion)").unwrap()
                < click.find("mousePressed").unwrap()
        );
        assert!(
            click.find("await moveVirtualCursor").unwrap()
                < click.find("assertPointerArrival(motion)").unwrap()
        );
    }

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const signatureEnd = source.indexOf(") {", start);
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      function deferred() {
        let resolve, reject;
        const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
        return { promise, resolve, reject };
      }
      const source = fs.readFileSync("extension/background.js", "utf8");
      const pointerPresentationState = extractFunction(source, "pointerPresentationState");
      const presentationBridge = new Function(`
        const POINTER_PRESENTATION_TIMEOUT_MS = 1_000;
        let tab = { id: 9, windowId: 4, active: true };
        let tabSequence = [];
        let targetWindow = { id: 4, focused: true, state: "normal" };
        let windowSequence = [];
        const chrome = {
          tabs: { get: async () => ({ ...(tabSequence.length ? tabSequence.shift() : tab) }) },
          windows: { get: async () => ({ ...(windowSequence.length ? windowSequence.shift() : targetWindow) }) },
        };
        function assertLeaseAuthority() {}
        function withCommandCancellation(promise) { return promise; }
        function withTimeout(promise) { return promise; }
        ${pointerPresentationState}
        return {
          read: () => pointerPresentationState(9, {}, null),
          set(nextTab, nextWindow) { tab = nextTab; targetWindow = nextWindow; },
          sequence(nextTabs) { tabSequence = nextTabs.map((item) => ({ ...item })); },
          windowSequence(nextWindows) { windowSequence = nextWindows.map((item) => ({ ...item })); },
        };
      `)();
      if (!(await presentationBridge.read()).animate) {
        throw new Error("active tab in a focused visible window was not classified for animation");
      }
      presentationBridge.set(
        { id: 9, windowId: 4, active: false },
        { id: 4, focused: true, state: "normal" },
      );
      if ((await presentationBridge.read()).reason !== "tab_inactive") {
        throw new Error("inactive tab was not routed to final-arrival movement");
      }
      presentationBridge.set(
        { id: 9, windowId: 4, active: true },
        { id: 4, focused: false, state: "normal" },
      );
      if ((await presentationBridge.read()).reason !== "window_unfocused") {
        throw new Error("unfocused window was not routed to final-arrival movement");
      }
      presentationBridge.set(
        { id: 9, windowId: 4, active: true },
        { id: 4, focused: true, state: "minimized" },
      );
      if ((await presentationBridge.read()).reason !== "window_minimized") {
        throw new Error("minimized window was not routed to final-arrival movement");
      }
      presentationBridge.set(
        { id: 9, windowId: 4, active: true },
        { id: 4, focused: true, state: "normal" },
      );
      presentationBridge.sequence([
        { id: 9, windowId: 4, active: true },
        { id: 9, windowId: 5, active: true },
      ]);
      if ((await presentationBridge.read()).reason !== "tab_presentation_changed") {
        throw new Error("tab window migration passed a stale window presentation sample");
      }
      presentationBridge.set(
        { id: 9, windowId: 4, active: true },
        { id: 4, focused: true, state: "normal" },
      );
      presentationBridge.windowSequence([
        { id: 4, focused: true, state: "normal" },
        { id: 4, focused: false, state: "normal" },
      ]);
      if ((await presentationBridge.read()).reason !== "window_presentation_changed") {
        throw new Error("window focus transition passed a stale presentation sample");
      }

      const moveVirtualCursor = extractFunction(source, "moveVirtualCursor");
      const bridge = new Function("deferred", `
        const lease = {
          tabId: 9, sessionId: "lease-9", epoch: 3, documentEpoch: 2,
          viewport: { width: 800, height: 600 }, moveSequence: 7, turn: 4,
          expiresAt: Date.now() + 30_000,
          cursor: { x: 20, y: 25, visible: true, updatedAt: Date.now() },
        };
        let animate = false;
        let deferOperations = true;
        let debuggerCalls = 0, contentCalls = 0, pauses = 0, persists = 0;
        const debuggerPoints = [], contentPoints = [];
        const debuggerGates = [], contentGates = [];
        function requireControl() { return Promise.resolve(lease); }
        function captureLeaseAuthority() {
          return { tabId: 9, sessionId: "lease-9", epoch: 3, documentEpoch: 2 };
        }
        function assertLeaseAuthority() {}
        function outcomeUnknownError(boundary, cause) {
          const error = new Error("ACTION_OUTCOME_UNKNOWN: " + boundary);
          error.code = "ACTION_OUTCOME_UNKNOWN";
          error.cause = cause;
          return error;
        }
        function clamp(value, minimum, maximum) { return Math.min(maximum, Math.max(minimum, value)); }
        function boundedBezierSpringPath(_start, target) {
          return {
            points: Array.from({ length: 12 }, (_, index) => ({
              x: target.x - 11 + index,
              y: target.y - 11 + index,
            })),
            durationMs: 192, distance: 140, candidateCount: 20, score: 1.5,
          };
        }
        function pointerPresentationState() {
          return Promise.resolve({
            animate, tabActive: animate, windowFocused: animate,
            windowState: animate ? "normal" : "normal",
            reason: animate ? "foreground_visible" : "tab_inactive",
          });
        }
        function debuggerCommand(_tabId, _method, params) {
          debuggerCalls += 1;
          debuggerPoints.push({ x: params.x, y: params.y });
          if (!deferOperations) return Promise.resolve();
          const gate = deferred();
          debuggerGates.push(gate);
          return gate.promise;
        }
        function contentRequest(_tabId, payload) {
          contentCalls += 1;
          contentPoints.push({ x: payload.cursor.x, y: payload.cursor.y });
          if (!deferOperations) return Promise.resolve();
          const gate = deferred();
          contentGates.push(gate);
          return gate.promise;
        }
        function pause() { pauses += 1; return Promise.resolve(); }
        function persistControlState() { persists += 1; return Promise.resolve(); }
        ${moveVirtualCursor}
        return {
          move: () => moveVirtualCursor(9, 400, 300),
          releaseDebugger: () => debuggerGates.shift().resolve(),
          releaseContent: () => contentGates.shift().resolve(),
          rejectContent: (error) => contentGates.shift().reject(error),
          foreground() {
            animate = true; deferOperations = false;
            debuggerCalls = 0; contentCalls = 0; pauses = 0; persists = 0;
            debuggerPoints.length = 0; contentPoints.length = 0;
          },
          transitioningForeground() {
            animate = true; deferOperations = true;
            debuggerCalls = 0; contentCalls = 0; pauses = 0; persists = 0;
            debuggerPoints.length = 0; contentPoints.length = 0;
          },
          failingForeground() {
            animate = true; deferOperations = true;
            debuggerCalls = 0; contentCalls = 0; pauses = 0; persists = 0;
            debuggerPoints.length = 0; contentPoints.length = 0;
          },
          background() { animate = false; },
          state: () => ({
            debuggerCalls, contentCalls, pauses, persists, cursor: lease.cursor,
            debuggerPoints: [...debuggerPoints], contentPoints: [...contentPoints],
          }),
        };
      `)(deferred);

      const backgroundMove = bridge.move();
      while (bridge.state().debuggerCalls === 0) await Promise.resolve();
      if (bridge.state().debuggerCalls !== 1 || bridge.state().contentCalls !== 0) {
        throw new Error("background movement dispatched more than its single final CDP move");
      }
      bridge.releaseDebugger();
      while (bridge.state().contentCalls === 0) await Promise.resolve();
      if (bridge.state().contentCalls !== 1) {
        throw new Error("background movement sent more than one cursor state");
      }
      bridge.releaseContent();
      const skipped = await backgroundMove;
      const backgroundState = bridge.state();
      if (skipped.arrival !== "skipped_background" || skipped.points !== 1
        || skipped.durationMs !== 0 || backgroundState.debuggerCalls !== 1
        || backgroundState.contentCalls !== 1 || backgroundState.pauses !== 0
        || backgroundState.persists !== 1 || backgroundState.cursor.x !== 400
        || backgroundState.cursor.y !== 300) {
        throw new Error("background movement did not acknowledge one persisted final arrival");
      }

      bridge.transitioningForeground();
      const transitionedMove = bridge.move();
      while (bridge.state().debuggerCalls === 0) await Promise.resolve();
      bridge.releaseDebugger();
      while (bridge.state().contentCalls === 0) await Promise.resolve();
      bridge.background();
      bridge.releaseContent();
      while (bridge.state().debuggerCalls < 2) await Promise.resolve();
      bridge.releaseDebugger();
      while (bridge.state().contentCalls < 2) await Promise.resolve();
      bridge.releaseContent();
      const transitioned = await transitionedMove;
      const transitionedState = bridge.state();
      const finalDebuggerPoint = transitionedState.debuggerPoints.at(-1);
      const finalContentPoint = transitionedState.contentPoints.at(-1);
      if (transitioned.arrival !== "skipped_background" || transitioned.points !== 2
        || transitionedState.debuggerCalls !== 2 || transitionedState.contentCalls !== 2
        || transitionedState.pauses !== 0 || finalDebuggerPoint.x !== 400
        || finalDebuggerPoint.y !== 300 || finalContentPoint.x !== 400
        || finalContentPoint.y !== 300) {
        throw new Error("foreground-to-background transition did not collapse to one final arrival");
      }

      bridge.failingForeground();
      const failedMove = bridge.move();
      while (bridge.state().debuggerCalls === 0) await Promise.resolve();
      bridge.releaseDebugger();
      while (bridge.state().contentCalls === 0) await Promise.resolve();
      const contentFailure = new Error("PAGE_UNAVAILABLE: renderer rejected the cursor update");
      contentFailure.code = "PAGE_UNAVAILABLE";
      bridge.rejectContent(contentFailure);
      let failureWasUnknown = false;
      let dependentClickStarted = false;
      try {
        await failedMove;
        dependentClickStarted = true;
      } catch (error) {
        failureWasUnknown = error.code === "ACTION_OUTCOME_UNKNOWN";
      }
      if (!failureWasUnknown || dependentClickStarted || bridge.state().debuggerCalls !== 1) {
        throw new Error("post-CDP cursor failure remained retryable or allowed the dependent click");
      }

      bridge.foreground();
      const arrived = await bridge.move();
      const foregroundState = bridge.state();
      if (arrived.arrival !== "arrived" || arrived.points !== 12
        || foregroundState.debuggerCalls !== 12 || foregroundState.contentCalls !== 12
        || foregroundState.pauses !== 11 || foregroundState.persists !== 1) {
        throw new Error("foreground movement did not retain the full animated path");
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run background pointer routing harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node background pointer routing harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn snapshots_invalidate_on_mutation_scroll_and_resize() {
    let content = extension_source("content.js");
    let core = extension_source("dom-core.js");
    assert!(core.contains("new MutationObserver"));
    assert!(core.contains("the document mutated"));
    assert!(core.contains("the page scrolled"));
    assert!(core.contains("the viewport resized"));
    assert!(content.contains("snapshotRevision !== revisions.read()"));
    assert!(content.contains("snapshotInvalidated: true"));
}

#[test]
fn snapshots_and_target_proofs_never_embed_live_text_input_values() {
    let content = extension_source("content.js");
    let core = extension_source("dom-core.js");
    let background = extension_source("background.js");
    assert!(core.contains("const safeInputValue = element instanceof HTMLInputElement"));
    assert!(core.contains("[\"button\", \"submit\", \"reset\"].includes(element.type)"));
    assert!(
        core.contains(
            "element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement"
        )
    );
    assert!(core.contains("isSensitiveFieldMetadata({ type, autocomplete, name: fieldName })"));
    assert!(!core.contains("![\"password\", \"hidden\"].includes(element.type)"));
    assert!(!content.contains("safeInputValue"));
    assert!(background.contains("isSensitiveField(description) || description.sensitive"));

    let script = r#"
      import fs from "node:fs";
      import { isSensitiveField } from "./extension/lib.js";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        const start = source.indexOf(marker);
        const signatureEnd = source.indexOf(") {", start);
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/dom-core.js", "utf8");
      const functions = [
        "clean", "normalizedFieldIdentifier", "isSensitiveFieldMetadata",
        "accessibleName", "targetSignature",
      ]
        .map((name) => extractFunction(source, name)).join("\n");
      const api = new Function(`
        class HTMLInputElement {
          constructor(type, value, attributes = {}) {
            this.type = type; this.value = value; this.attributes = attributes;
            this.labels = []; this.alt = ""; this.title = ""; this.placeholder = "";
            this.innerText = ""; this.textContent = ""; this.id = ""; this.tagName = "INPUT";
          }
          getAttribute(name) { return this.attributes[name] ?? null; }
        }
        class HTMLTextAreaElement {
          constructor(value, attributes = {}) {
            this.value = value; this.attributes = attributes;
            this.labels = []; this.alt = ""; this.title = ""; this.placeholder = "";
            this.innerText = value; this.textContent = value; this.id = ""; this.tagName = "TEXTAREA";
          }
          getAttribute(name) { return this.attributes[name] ?? null; }
        }
        class HTMLAnchorElement {}
        function labelledBy() { return ""; }
        function roleOf() { return "textbox"; }
        ${functions}
        return {
          HTMLInputElement, HTMLTextAreaElement, accessibleName, targetSignature,
          isSensitiveFieldMetadata,
        };
      `)();
      const cases = [
        ["password", "password", "secret-password"],
        ["text", "one-time-code", "123456"],
        ["text", "cc-number", "4111111111111111"],
        ["text", "cc-csc", "999"],
        ["text", "current-password", "current-secret"],
        ["text", "new-password", "new-secret"],
      ];
      for (const [type, autocomplete, secret] of cases) {
        const input = new api.HTMLInputElement(type, secret, {
          autocomplete,
          name: autocomplete,
          "aria-disabled": "false",
          "aria-checked": "false",
        });
        if (api.accessibleName(input).includes(secret)
          || api.targetSignature(input).includes(secret)) {
          throw new Error(`${autocomplete} live value leaked into observation metadata`);
        }
      }
      const labeled = new api.HTMLInputElement("text", "private-code", {
        "aria-label": "Verification code",
      });
      if (api.accessibleName(labeled) !== "Verification code") {
        throw new Error("non-secret accessible label was not preserved");
      }
      const submit = new api.HTMLInputElement("submit", "Continue");
      if (api.accessibleName(submit) !== "Continue") {
        throw new Error("safe submit-button value label was removed");
      }
      const draft = new api.HTMLTextAreaElement("private medical draft", {
        "aria-label": "Notes",
        name: "notes",
        "aria-disabled": "false",
        "aria-checked": "false",
      });
      if (api.accessibleName(draft) !== "Notes"
        || api.targetSignature(draft).includes("private medical draft")) {
        throw new Error("textarea live text leaked or its non-secret label was removed");
      }

      const standardAutocomplete = [
        "current-password", "new-password", "one-time-code", "cc-name",
        "cc-given-name", "cc-additional-name", "cc-family-name", "cc-number",
        "cc-exp", "cc-exp-month", "cc-exp-year", "cc-csc", "cc-type",
      ];
      for (const autocomplete of standardAutocomplete) {
        const metadata = { type: "text", autocomplete, name: "ordinary" };
        if (!api.isSensitiveFieldMetadata(metadata) || !isSensitiveField(metadata)) {
          throw new Error(`${autocomplete} was not classified as sensitive`);
        }
      }
      const sensitiveAliases = [
        "cc-name", "cc-number", "cc-exp", "cc-exp-month", "cc-exp-year", "cc-csc",
        "cardNumber", "card_number", "creditCardNumber", "cardholderName",
        "cardExpiry", "cardSecurityCode", "cvv", "cvv2", "cvc", "cvc2",
        "otp", "passcode", "verificationCode", "oneTimeCode",
      ];
      for (const name of sensitiveAliases) {
        const metadata = { type: "text", autocomplete: "", name };
        if (!api.isSensitiveFieldMetadata(metadata) || !isSensitiveField(metadata)
          || !isSensitiveField({ fieldName: name })) {
          throw new Error(`${name} was not classified as sensitive`);
        }
      }
      for (const name of ["username", "email", "postalCode", "promoCode", "search", "phone", "companyName"]) {
        const metadata = { type: "text", autocomplete: "", name };
        if (api.isSensitiveFieldMetadata(metadata) || isSensitiveField(metadata)) {
          throw new Error(`${name} was incorrectly classified as sensitive`);
        }
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run sensitive observation harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node sensitive observation harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn safe_mode_blocks_sensitive_selects_before_page_mutation() {
    let background = extension_source("background.js");
    let content = extension_source("content.js");
    let select_start = background.find("case \"page.select\"").unwrap();
    let select_end = background[select_start + 5..]
        .find("\n    case \"")
        .map(|offset| select_start + 5 + offset)
        .unwrap_or(background.len());
    let select = &background[select_start..select_end];
    assert!(select.contains("method: \"describe\""));
    assert!(select.contains("isSensitiveField(description) || description.sensitive"));
    assert!(select.contains("allowSensitive: config.fullAccess"));
    assert!(
        select
            .find("isSensitiveField(description) || description.sensitive")
            .unwrap()
            < select.find("method: \"select\"").unwrap()
    );

    let content_select_start = content.find("case \"select\"").unwrap();
    let content_select_end = content[content_select_start..]
        .find("case \"scroll\"")
        .map(|offset| content_select_start + offset)
        .unwrap_or(content.len());
    let content_select = &content[content_select_start..content_select_end];
    assert!(content_select.contains("description.sensitive && !message.allowSensitive"));
    assert!(
        content_select
            .find("description.sensitive && !message.allowSensitive")
            .unwrap()
            < content_select.find("element.value = option.value").unwrap()
    );

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const signatureEnd = source.indexOf(") {", start);
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/content.js", "utf8");
      const functions = ["assertActiveControl", "handle"]
        .map((name) => extractFunction(source, name)).join("\n");
      const bridge = new Function(`
        let activeControlSessionId = "control-session";
        let activeControlEpoch = 4;
        let sensitive = true;
        let dispatched = 0;
        class HTMLSelectElement {
          constructor() {
            this.value = "old";
            this.options = [{ value: "12", label: "December" }];
          }
          dispatchEvent() { dispatched += 1; }
        }
        class Event { constructor(type, options) { this.type = type; this.options = options; } }
        const element = new HTMLSelectElement();
        function resolveRecord() { return { element }; }
        function validateRecord() { return { description: { sensitive } }; }
        ${functions}
        return {
          select: (allowSensitive) => handle({
            method: "select", controlSessionId: "control-session", controlEpoch: 4,
            ref: "e1", generation: "g1", value: "12", allowSensitive,
          }),
          markOrdinary: () => { sensitive = false; },
          state: () => ({ value: element.value, dispatched }),
        };
      `)();
      let blocked = false;
      try { await bridge.select(false); }
      catch (error) { blocked = String(error.message).includes("SENSITIVE_FIELD"); }
      if (!blocked || bridge.state().value !== "old" || bridge.state().dispatched !== 0) {
        throw new Error("Safe mode mutated a sensitive payment-card select");
      }
      bridge.markOrdinary();
      await bridge.select(false);
      if (bridge.state().value !== "12" || bridge.state().dispatched !== 2) {
        throw new Error("an ordinary select was incorrectly blocked");
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run sensitive select harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node sensitive select harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn control_is_visible_and_user_stoppable_in_page_and_popup() {
    let content = extension_source("content.js");
    let popup = extension_source("popup.html");
    let popup_script = extension_source("popup.js");

    assert!(content.contains("Local Browser Bridge is using this tab"));
    assert!(content.contains("setAttribute(\"popover\", \"manual\")"));
    assert!(content.contains("showPopover"));
    assert!(content.contains("LBB_CONTROL_UI"));
    assert!(content.contains("stop.textContent = \"Stop\""));
    assert!(content.contains("lastControlState"));
    assert!(content.contains("!controlUi.host.isConnected"));
    assert!(content.contains("queueMicrotask"));
    assert!(popup.contains("Chrome shows its native debugging notice"));
    assert!(popup.contains("id=\"start-control\""));
    assert!(popup.contains("id=\"release-control\""));
    assert!(popup_script.contains("startControlCurrent"));
    assert!(popup_script.contains("releaseControl"));
    assert!(content.contains("CONTROL_LAST_SEEN_GRACE_MS"));
    assert!(content.contains("controlExpiresAt"));
    assert!(content.contains("controlLastSeenAt"));
    assert!(content.contains("expireStaleControl"));
    assert!(content.contains("addEventListener(\"pageshow\""));
    assert!(content.contains("action: \"reconcile\""));
}

#[test]
fn capture_visibility_survives_overlay_reinsertion_and_nested_capture() {
    let background = extension_source("background.js");
    let content = extension_source("content.js");
    assert!(background.contains("const captureId = crypto.randomUUID()"));
    assert!(background.contains("method: \"control.capture.begin\", captureId"));
    assert!(background.contains("method: \"control.capture.end\""));
    assert!(background.contains("activeCaptureIds: remainingCaptureIds"));
    assert!(
        background
            .contains("controlUiAcknowledged(beginState, activeCaptureIds, lease.cursor.visible)")
    );
    assert!(
        background
            .contains("controlUiAcknowledged(endState, remainingCaptureIds, lease.cursor.visible)")
    );
    assert!(background.contains("controlSessionId: authority.sessionId"));
    assert!(background.contains("controlEpoch: authority.epoch"));
    assert!(background.contains("}, { authority, commandContext });"));
    assert!(background.contains("await showControlUi(controlLease)"));
    assert!(content.contains("captureDepth"));
    assert!(content.contains("__LOCAL_BROWSER_BRIDGE_CAPTURE_DEPTH__"));
    assert!(content.contains("activeCaptureIds"));
    assert!(content.contains("captureStartedAt"));
    assert!(content.contains("reconcileCaptureIds(message.activeCaptureIds)"));
    assert!(content.contains("expireStaleCaptures"));
    assert!(content.contains("applyCaptureVisibility()"));
    assert!(content.contains("captureDepth > 0 ? \"hidden\" : \"visible\""));
    assert!(content.contains("controlUi.cursor.style.visibility = \"visible\""));
    assert!(background.contains("state.cursorVisible !== Boolean(expectedCursorVisible)"));

    let capture_end_case = content.find("case \"control.capture.end\"").unwrap();
    let capture_end_body = &content[capture_end_case
        ..content[capture_end_case..]
            .find("default:")
            .map(|offset| capture_end_case + offset)
            .unwrap_or(content.len())];
    assert!(
        capture_end_body
            .find("assertActiveControl(message.controlSessionId, message.controlEpoch)")
            .unwrap()
            < capture_end_body.find("setCaptureMode(false").unwrap()
    );

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const signatureEnd = source.indexOf(") {", start);
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/content.js", "utf8");
      const functions = ["assertActiveControl", "setCaptureMode", "handle"]
        .map((name) => extractFunction(source, name)).join("\n");
      const bridge = new Function(`
        let activeControlSessionId = "session-a";
        let activeControlEpoch = 1;
        const activeCaptureIds = new Set();
        const captureStartedAt = new Map();
        function reconcileCaptureIds(ids) {
          activeCaptureIds.clear();
          for (const id of ids || []) activeCaptureIds.add(String(id));
        }
        function syncCaptureGlobals() {}
        async function confirmControlUiPaint() {
          return { activeCaptureIds: [...activeCaptureIds] };
        }
        ${functions}
        return {
          handle,
          switchLease(sessionId, epoch) {
            activeControlSessionId = sessionId;
            activeControlEpoch = epoch;
            activeCaptureIds.clear();
          },
          captures: () => [...activeCaptureIds],
        };
      `)();
      await bridge.handle({
        method: "control.capture.begin", controlSessionId: "session-a",
        controlEpoch: 1, captureId: "capture-a", activeCaptureIds: ["capture-a"],
      });
      bridge.switchLease("session-b", 2);
      await bridge.handle({
        method: "control.capture.begin", controlSessionId: "session-b",
        controlEpoch: 2, captureId: "capture-b", activeCaptureIds: ["capture-b"],
      });
      let rejected = false;
      try {
        await bridge.handle({
          method: "control.capture.end", controlSessionId: "session-a",
          controlEpoch: 1, captureId: "capture-a", activeCaptureIds: [],
        });
      } catch (error) {
        rejected = String(error.message).includes("CONTROL_REVOKED");
      }
      if (!rejected || bridge.captures().join() !== "capture-b") {
        throw new Error("a stale capture finalizer altered the replacement lease capture set");
      }
      await bridge.handle({
        method: "control.capture.end", controlSessionId: "session-b",
        controlEpoch: 2, captureId: "capture-b", activeCaptureIds: [],
      });
      if (bridge.captures().length !== 0) throw new Error("the active lease could not end its capture");
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run capture lease harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node capture lease harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn only_bridge_created_tabs_are_grouped() {
    let background = extension_source("background.js");
    let grouping_start = background
        .find("async function groupBridgeCreatedTab")
        .unwrap();
    let grouping_end = background[grouping_start..]
        .find("async function dispatch")
        .unwrap()
        + grouping_start;
    let grouping = &background[grouping_start..grouping_end];
    assert!(grouping.contains("chrome.tabs.group({ tabIds: tab.id })"));
    assert!(grouping.contains("chrome.tabGroups.update"));
    assert!(grouping.contains("Local Browser Bridge"));
    assert_eq!(background.matches("chrome.tabs.group(").count(), 1);

    let new_start = background.find("case \"tabs.new\"").unwrap();
    let new_end = background[new_start + 5..].find("\n    case \"").unwrap() + new_start + 5;
    assert!(background[new_start..new_end].contains("groupBridgeCreatedTab(tab, commandContext)"));
}

#[test]
fn tabs_new_validates_an_optional_url_before_creation() {
    let background = extension_source("background.js");
    let validation_start = background
        .find("async function createAllowedBridgeTab")
        .unwrap();
    let validation_end = background[validation_start..]
        .find("async function createBridgeTab")
        .unwrap()
        + validation_start;
    let validation = &background[validation_start..validation_end];
    assert!(validation.contains("typeof rawUrl !== \"string\""));
    assert!(validation.contains("NEW_TAB_URL_MAX_CHARS"));
    assert!(
        validation.find("await settings()").unwrap() < validation.find("isUrlAllowed(").unwrap()
    );
    assert!(
        validation.find("isUrlAllowed(").unwrap() < validation.find("createBridgeTab(").unwrap()
    );
    assert!(validation.contains("creationUrl = verdict.url"));

    let new_start = background.find("case \"tabs.new\"").unwrap();
    let new_end = background[new_start + 5..].find("\n    case \"").unwrap() + new_start + 5;
    let new_case = &background[new_start..new_end];
    assert!(new_case.contains("createAllowedBridgeTab(params.url, commandContext)"));
    assert!(!new_case.contains("chrome.tabs.create"));

    let script = r#"
      import fs from "node:fs";
      import { isUrlAllowed as actualIsUrlAllowed } from "./extension/lib.js";

      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (start < 0) throw new Error(`missing ${name}`);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const brace = source.indexOf("{", start);
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }

      const source = fs.readFileSync("extension/background.js", "utf8");
      const createAllowedBridgeTab = extractFunction(source, "createAllowedBridgeTab");
      const events = [];
      const config = {
        allowedHosts: ["allowed.example", "localhost", "127.0.0.1"],
        port: 17373,
        fullAccess: false,
      };
      const create = new Function("actualIsUrlAllowed", "events", "config", `
        const NEW_TAB_URL_MAX_CHARS = 4096;
        async function settings() {
          events.push(["settings"]);
          return config;
        }
        function isUrlAllowed(...args) {
          events.push(["policy", args[0]]);
          return actualIsUrlAllowed(...args);
        }
        async function createBridgeTab(commandContext, creationUrl) {
          events.push(["create", creationUrl, commandContext]);
          return { id: 41 };
        }
        ${createAllowedBridgeTab}
        return createAllowedBridgeTab;
      `)(actualIsUrlAllowed, events, config);

      const omitted = await create(undefined, "blank-context");
      if (omitted.id !== 41 || JSON.stringify(events) !== JSON.stringify([
        ["create", "about:blank", "blank-context"],
      ])) throw new Error(`omitted URL changed legacy creation: ${JSON.stringify(events)}`);

      events.length = 0;
      await create("HTTPS://ALLOWED.EXAMPLE:443/path?private=query#fragment", "url-context");
      if (JSON.stringify(events) !== JSON.stringify([
        ["settings"],
        ["policy", "HTTPS://ALLOWED.EXAMPLE:443/path?private=query#fragment"],
        ["create", "https://allowed.example/path?private=query#fragment", "url-context"],
      ])) throw new Error(`allowed URL was not canonicalized before creation: ${JSON.stringify(events)}`);

      const unicodePrefix = "https://allowed.example/";
      const unicodeBoundary = unicodePrefix + "😀".repeat(4096 - [...unicodePrefix].length);
      events.length = 0;
      await create(unicodeBoundary, "unicode-context");
      const unicodeCreate = events.find(([event]) => event === "create");
      if (unicodeCreate?.[1] !== new URL(unicodeBoundary).href
        || unicodeCreate?.[2] !== "unicode-context") {
        throw new Error("the 4096-code-point Unicode URL boundary did not create canonically");
      }

      for (const blockedUrl of [
        "",
        "http://127.0.0.1:17373/api/v1/command",
        "https://blocked.example/path",
        "javascript:alert(1)",
      ]) {
        events.length = 0;
        let blocked = false;
        let failure = "";
        try { await create(blockedUrl, "blocked-context"); }
        catch (error) {
          failure = String(error.message);
          blocked = /^(SITE_BLOCKED|BAD_URL):/.test(failure);
        }
        if (!blocked) throw new Error(`${blockedUrl} was not blocked`);
        if (blockedUrl === "" && !failure.startsWith("BAD_URL:")) {
          throw new Error("an empty URL did not fail as BAD_URL");
        }
        if (events.some(([event]) => event === "create")) {
          throw new Error(`${blockedUrl} reached tab creation`);
        }
      }

      const overlongUnicode = unicodePrefix + "😀".repeat(4097 - [...unicodePrefix].length);
      for (const invalid of [null, true, 7, {}, "x".repeat(4097), overlongUnicode]) {
        events.length = 0;
        let blocked = false;
        try { await create(invalid, "invalid-context"); }
        catch (error) { blocked = String(error.message).startsWith("BAD_URL:"); }
        if (!blocked || events.length !== 0) {
          throw new Error(`invalid URL shape crossed validation: ${JSON.stringify(events)}`);
        }
      }
    "#;
    let output = Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
        .expect("failed to run tabs.new URL harness");
    assert!(
        output.status.success(),
        "Node tabs.new URL harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn server_cancel_is_session_bound_suppresses_late_results_and_preserves_explicit_lease() {
    let background = extension_source("background.js");
    let freshness_methods = background
        .split("const REQUEST_CANCEL_FRESHNESS_METHODS = new Set([")
        .nth(1)
        .unwrap()
        .split("]);")
        .next()
        .unwrap();
    for method in [
        "browser.control.start",
        "page.observe",
        "page.navigate",
        "page.back",
        "page.forward",
        "page.reload",
        "page.click",
        "page.fill",
        "page.select",
        "page.key",
        "page.scroll",
        "page.clickAt",
        "page.typeText",
        "page.evaluate",
        "page.hover",
        "page.batch",
        "page.handleDialog",
    ] {
        assert!(
            freshness_methods.contains(&format!("\"{method}\"")),
            "{method} is missing from request-cancel freshness invalidation"
        );
    }
    for method in [
        "status",
        "tabs.list",
        "tabs.activate",
        "tabs.new",
        "tabs.close",
        "browser.control.status",
        "browser.control.stop",
        "page.waitFor",
    ] {
        assert!(
            !freshness_methods.contains(&format!("\"{method}\"")),
            "{method} must not mutate controlled-page freshness"
        );
    }
    let session_guard = background
        .find("message.sessionId !== protocolServerSessionId")
        .unwrap();
    let cancel_start = background.find("if (message.type === \"cancel\")").unwrap();
    let command_start = background[cancel_start..]
        .find("if (message.type !== \"command\"")
        .unwrap()
        + cancel_start;
    assert!(session_guard < cancel_start);
    assert!(cancel_start < command_start);

    let cancel = &background[cancel_start..command_start];
    assert!(cancel.contains("Number.isSafeInteger(message.sequence)"));
    assert!(cancel.contains("commandKey(protocolServerSessionId, message.id, message.sequence)"));
    assert!(cancel.contains("rememberCanceledCommand(key)"));
    assert!(cancel.contains("activeCommandContexts.get(key)"));
    assert!(cancel.contains("message.reason === \"request_canceled\""));
    assert!(cancel.contains(
        "cancelCommandContext(context, requestCanceled ? \"request_canceled\" : \"server_timeout\")"
    ));
    assert!(
        cancel
            .contains("context.cancellationCleanup = invalidateCanceledCommandFreshness(context)")
    );
    assert!(cancel.contains("else if (context.started && commandUsesControl(context.method)"));
    assert!(cancel.contains("stopControl(\"command_canceled\", { requireExplicitStart: true })"));

    let chain_start = background[command_start..]
        .find("commandChain = commandChain.then")
        .unwrap()
        + command_start;
    let chain_end = background[chain_start..]
        .find("nextSocket.onerror")
        .unwrap()
        + chain_start;
    let chain = &background[chain_start..chain_end];
    assert!(chain.contains("dispatch(message.method, message.params ?? {}, false, context)"));
    assert!(chain.contains("assertCommandActive(context, \"result delivery\")"));
    assert!(chain.contains("if (context.canceled || canceledCommandKeys.has(context.key)) return"));
    assert!(chain.contains("await context.cancellationCleanup"));
    assert!(chain.contains("finalizeCanceledCommandFreshness(context)"));
    assert!(chain.contains("activeCommandContexts.delete(context.key)"));
    assert!(background.contains("context.cancelWaiters.add(rejectCancellation)"));
    assert!(background.contains("for (const reject of context.cancelWaiters ?? [])"));
}

#[test]
fn canceled_deferred_cdp_mutation_invalidates_turn_and_generation_before_recovery() {
    let script = r#"
      import fs from "node:fs";

      function extractFunction(source, name) {
        const asyncMarker = `async function ${name}(`;
        const plainMarker = `function ${name}(`;
        const start = source.includes(asyncMarker)
          ? source.indexOf(asyncMarker)
          : source.indexOf(plainMarker);
        if (start < 0) throw new Error(`missing ${name}`);
        const brace = source.indexOf("{", start);
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }

      function deferred() {
        let resolve, reject;
        const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
        return { promise, resolve, reject };
      }

      const source = fs.readFileSync("extension/background.js", "utf8");
      const functions = [
        "requestCancelInvalidatesFreshness", "rememberCanceledCommand",
        "commandCanceledError", "assertCommandActive", "withCommandCancellation",
        "cancelCommandContext", "persistCanceledCommandFreshness",
        "invalidateCanceledCommandFreshness", "assertControlBinding",
        "assertFrameSnapshotFresh", "insertText", "observeControlledPage",
      ].map((name) => extractFunction(source, name)).join("\n");

      const bridge = new Function("deferred", `
        const REQUEST_CANCEL_FRESHNESS_METHODS = new Set([
          "browser.control.start", "page.observe", "page.navigate", "page.back",
          "page.forward", "page.reload", "page.click", "page.fill", "page.select",
          "page.key", "page.scroll", "page.clickAt", "page.typeText",
          "page.evaluate", "page.hover", "page.batch", "page.handleDialog",
        ]);
        const canceledCommandKeys = new Set();
        const attachedFrames = new Map();
        const frameSkips = [];
        let frameTreeRevision = 4;
        let frameInvalidationReason = "";
        let frameSnapshot = { generation: "g-old", frameTreeRevision, frames: new Map() };
        let controlEpoch = 7;
        let controlLease = {
          sessionId: "lease-7", epoch: 7, tabId: 17, turn: 11,
          moveSequence: 2, viewport: { width: 800, height: 600 },
        };
        let fieldValue = "";
        let cdpDispatched = false;
        const cdpGate = deferred();
        const firstPersist = deferred();
        let persistCount = 0;
        const events = [];
        let revocations = 0;

        function persistControlState() {
          persistCount += 1;
          return persistCount === 1 ? firstPersist.promise : Promise.resolve();
        }
        function stopControl() { revocations += 1; controlEpoch += 1; controlLease = null; return Promise.resolve(); }
        function send(message) { events.push(structuredClone(message)); return true; }
        function publicControlState() {
          return controlLease
            ? { active: true, sessionId: controlLease.sessionId, tabId: controlLease.tabId,
                turn: controlLease.turn, moveSequence: controlLease.moveSequence }
            : { active: false };
        }
        function requireControl() { return Promise.resolve(controlLease); }
        function captureLeaseAuthority(lease = controlLease) {
          return { tabId: lease.tabId, sessionId: lease.sessionId, epoch: lease.epoch };
        }
        function assertLeaseAuthority(authority, context, boundary) {
          assertCommandActive(context, boundary);
          if (!controlLease || authority.sessionId !== controlLease.sessionId
            || authority.epoch !== controlLease.epoch || authority.epoch !== controlEpoch) {
            throw new Error("CONTROL_CANCELED");
          }
        }
        function verifyDocumentAuthority() { return Promise.resolve(); }
        function verifyDocumentAuthorityAfterDispatch() { return Promise.resolve(); }
        function debuggerCommand(tabId, method, params, authority, context) {
          if (method !== "Input.insertText") throw new Error("unexpected CDP method " + method);
          assertLeaseAuthority(authority, context, "CDP text dispatch");
          // Chrome has accepted the side effect, but its command promise is
          // deliberately still pending when cancellation arrives.
          fieldValue += params.text;
          cdpDispatched = true;
          return withCommandCancellation(cdpGate.promise, context, "deferred Input.insertText");
        }
        function assertDialogNotBlocking() {}
        function showControlUi() { return Promise.resolve(); }
        function contentRequest(tabId, message) {
          if (message.method !== "snapshot") throw new Error("unexpected content request");
          return Promise.resolve({
            generation: "g-fresh", title: "Fresh", url: "https://example.test/",
            viewport: { width: 800, height: 600 }, elements: [], frameOwners: [],
          });
        }
        function collectFrameObservations(tabId, snapshot) {
          return Promise.resolve({ elements: snapshot.elements, frames: [], frameSummary: {} });
        }
        function rethrowLeaseFatalFrameError(error) { throw error; }
        function mergeFrameObservations(snapshot) {
          return { elements: snapshot.elements, frames: [], frameSummary: {} };
        }
        function frameObservationIsSilent() { return true; }
        function captureTab() { return Promise.resolve({ dataUrl: "fixture" }); }
        function classifyRisk() { return "low"; }
        ${functions}
        return {
          startType(context) { return insertText(17, "X", context); },
          cancel(context) {
            cancelCommandContext(context, "request_canceled");
            context.cancellationCleanup = invalidateCanceledCommandFreshness(context);
          },
          oldBindingsFail() {
            let staleTurn = false, staleGeneration = false;
            try { assertControlBinding({ controlSessionId: "lease-7", turn: 11, moveSequence: 2 }); }
            catch (error) { staleTurn = error.message.startsWith("STALE_CONTROL_TURN:"); }
            try { assertFrameSnapshotFresh("g-old"); }
            catch (error) { staleGeneration = error.message.startsWith("STALE_SNAPSHOT:"); }
            return staleTurn && staleGeneration;
          },
          observe(context) { return observeControlledPage({ id: 17 }, context); },
          freshBindingsPass() {
            assertControlBinding({ controlSessionId: "lease-7", turn: 13, moveSequence: 2 });
            assertFrameSnapshotFresh("g-fresh");
          },
          releaseCdp() { cdpGate.resolve({}); },
          releasePersist() { firstPersist.resolve(); },
          state() { return {
            fieldValue, cdpDispatched, turn: controlLease?.turn, frame: frameSnapshot?.generation ?? null,
            persistCount, events: structuredClone(events), revocations,
          }; },
        };
      `)(deferred);

      const context = {
        key: "server-session:9:type-text", method: "page.typeText",
        started: true, canceled: false, cancelWaiters: new Set(),
        cancellationCleanup: Promise.resolve(false),
      };
      const pending = bridge.startType(context);
      while (!bridge.state().cdpDispatched) await Promise.resolve();
      if (bridge.state().fieldValue !== "X") throw new Error("fixture did not mutate before cancel");

      bridge.cancel(context);
      let nextQueuedCommandDispatched = false;
      const nextQueuedCommand = context.cancellationCleanup.then(() => {
        nextQueuedCommandDispatched = true;
      });
      const immediate = bridge.state();
      if (immediate.turn !== 12 || immediate.frame !== null || !bridge.oldBindingsFail()) {
        throw new Error(`cancel did not synchronously invalidate stale authority: ${JSON.stringify(immediate)}`);
      }
      if (immediate.events.length !== 0) throw new Error("freshness event published before durable turn");
      if (nextQueuedCommandDispatched) throw new Error("next queued command passed the persistence barrier");

      let canceled = false;
      try { await pending; }
      catch (error) { canceled = error.code === "COMMAND_CANCELED"; }
      if (!canceled) throw new Error("deferred Input.insertText was not canceled");
      bridge.releaseCdp();
      bridge.releasePersist();
      await context.cancellationCleanup;
      await nextQueuedCommand;
      if (!nextQueuedCommandDispatched) throw new Error("queued command did not resume after persistence");
      const invalidated = bridge.state();
      if (invalidated.events.length !== 1
        || invalidated.events[0].name !== "browser.control.freshness_invalidated"
        || invalidated.events[0].data.control.turn !== 12
        || invalidated.revocations !== 0) {
        throw new Error(`durable freshness event was wrong: ${JSON.stringify(invalidated)}`);
      }

      const observed = await bridge.observe({
        key: "server-session:10:observe", method: "page.observe",
        started: true, canceled: false, cancelWaiters: new Set(),
      });
      if (observed.snapshot.generation !== "g-fresh" || observed.control.turn !== 13) {
        throw new Error(`explicit observation did not recover freshness: ${JSON.stringify(observed)}`);
      }
      bridge.freshBindingsPass();

      const failureFunctions = [
        "requestCancelInvalidatesFreshness", "persistCanceledCommandFreshness",
        "invalidateCanceledCommandFreshness",
      ].map((name) => extractFunction(source, name)).join("\n");
      const failingPersistence = new Function(`
        const REQUEST_CANCEL_FRESHNESS_METHODS = new Set(["page.typeText"]);
        let controlEpoch = 3;
        let controlLease = { sessionId: "fail-lease", epoch: 3, tabId: 3, turn: 5 };
        let frameSnapshot = { generation: "fail-old" };
        let frameInvalidationReason = "";
        let stopCalls = 0;
        const events = [];
        function persistControlState() { return Promise.reject(new Error("storage unavailable")); }
        function stopControl() { stopCalls += 1; controlEpoch += 1; controlLease = null; return Promise.resolve(); }
        function send(message) { events.push(message); return true; }
        function publicControlState() { return controlLease ? { active: true, turn: controlLease.turn } : { active: false }; }
        ${failureFunctions}
        return {
          run() { return invalidateCanceledCommandFreshness({ started: true, method: "page.typeText" }); },
          state() { return { active: Boolean(controlLease), stopCalls, events: events.length, frame: frameSnapshot }; },
        };
      `)();
      await failingPersistence.run();
      const failed = failingPersistence.state();
      if (failed.active || failed.stopCalls !== 1 || failed.events !== 0 || failed.frame !== null) {
        throw new Error(`failed freshness persistence did not revoke: ${JSON.stringify(failed)}`);
      }
    "#;

    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run deferred CDP cancellation harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Deferred CDP cancellation harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn canceled_observe_cannot_republish_a_frame_snapshot_after_its_persist_await() {
    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        for (const marker of [`async function ${name}(`, `function ${name}(`]) {
          const start = source.indexOf(marker);
          if (start < 0) continue;
          const brace = source.indexOf("{", start);
          let depth = 0, quote = "", escaped = false;
          for (let index = brace; index < source.length; index += 1) {
            const character = source[index];
            if (quote) {
              if (escaped) escaped = false;
              else if (character === "\\") escaped = true;
              else if (character === quote) quote = "";
            } else if (["\"", "'", "`"].includes(character)) quote = character;
            else if (character === "{") depth += 1;
            else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
          }
        }
        throw new Error(`missing ${name}`);
      }
      function deferred() {
        let resolve;
        const promise = new Promise((yes) => { resolve = yes; });
        return { promise, resolve };
      }
      const source = fs.readFileSync("extension/background.js", "utf8");
      const functions = [
        "requestCancelInvalidatesFreshness", "rememberCanceledCommand",
        "commandCanceledError", "assertCommandActive", "cancelCommandContext",
        "persistCanceledCommandFreshness", "invalidateCanceledCommandFreshness",
        "finalizeCanceledCommandFreshness", "observeControlledPage",
      ].map((name) => extractFunction(source, name)).join("\n");
      const bridge = new Function("deferred", `
        const REQUEST_CANCEL_FRESHNESS_METHODS = new Set(["page.observe"]);
        const canceledCommandKeys = new Set();
        const attachedFrames = new Map();
        const frameSkips = [];
        let frameTreeRevision = 2;
        let frameInvalidationReason = "";
        let frameSnapshot = { generation: "g-old", frames: new Map() };
        let controlEpoch = 4;
        let controlLease = { sessionId: "lease", epoch: 4, tabId: 7, turn: 4 };
        const observationPersist = deferred();
        const cancellationPersist = deferred();
        let persistCalls = 0;
        function persistControlState() {
          persistCalls += 1;
          return persistCalls === 1 ? observationPersist.promise : cancellationPersist.promise;
        }
        function stopControl() { controlLease = null; controlEpoch += 1; return Promise.resolve(); }
        function send() { return true; }
        function publicControlState() { return { active: true, turn: controlLease.turn }; }
        function assertDialogNotBlocking() {}
        function requireControl() { return Promise.resolve(controlLease); }
        function captureLeaseAuthority() { return { tabId: 7, sessionId: "lease", epoch: 4 }; }
        function assertLeaseAuthority(authority, context, boundary) {
          assertCommandActive(context, boundary);
          if (!controlLease || authority.sessionId !== controlLease.sessionId
            || authority.epoch !== controlLease.epoch || authority.epoch !== controlEpoch) {
            throw new Error("CONTROL_CANCELED");
          }
        }
        function verifyDocumentAuthority() { return Promise.resolve(); }
        function showControlUi() { return Promise.resolve(); }
        function contentRequest() {
          return Promise.resolve({
            generation: "g-canceled", viewport: { width: 800, height: 600 },
            elements: [], frameOwners: [],
          });
        }
        function collectFrameObservations(tabId, snapshot) {
          return Promise.resolve({ elements: snapshot.elements, frames: [], frameSummary: {} });
        }
        function rethrowLeaseFatalFrameError(error) { throw error; }
        function mergeFrameObservations(snapshot) {
          return { elements: snapshot.elements, frames: [], frameSummary: {} };
        }
        function frameObservationIsSilent() { return true; }
        function captureTab() { throw new Error("capture must not run after cancellation"); }
        function classifyRisk() { return "low"; }
        ${functions}
        return {
          observe(context) { return observeControlledPage({ id: 7 }, context); },
          cancel(context) {
            cancelCommandContext(context, "request_canceled");
            context.cancellationCleanup = invalidateCanceledCommandFreshness(context);
          },
          releaseObservationPersist() { observationPersist.resolve(); },
          releaseCancellationPersist() { cancellationPersist.resolve(); },
          finalize(context) { finalizeCanceledCommandFreshness(context); },
          state() { return { persistCalls, turn: controlLease?.turn, frame: frameSnapshot?.generation ?? null }; },
        };
      `)(deferred);

      const context = {
        key: "session:observe", method: "page.observe", started: true,
        canceled: false, cancelWaiters: new Set(), cancellationCleanup: Promise.resolve(false),
      };
      const pending = bridge.observe(context);
      while (bridge.state().persistCalls < 1) await Promise.resolve();
      bridge.cancel(context);
      if (bridge.state().turn !== 6 || bridge.state().frame !== null) {
        throw new Error(`cancel did not invalidate observe immediately: ${JSON.stringify(bridge.state())}`);
      }
      bridge.releaseObservationPersist();
      let canceled = false;
      try { await pending; } catch (error) { canceled = error.code === "COMMAND_CANCELED"; }
      if (!canceled || bridge.state().frame !== "g-canceled") {
        throw new Error(`fixture did not reproduce post-cancel frame reassignment: ${JSON.stringify(bridge.state())}`);
      }
      bridge.releaseCancellationPersist();
      await context.cancellationCleanup;
      bridge.finalize(context);
      if (bridge.state().frame !== null || bridge.state().turn !== 6) {
        throw new Error(`final queue barrier did not re-clear without a second turn bump: ${JSON.stringify(bridge.state())}`);
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run canceled-observe race harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Canceled-observe race harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn canceled_tab_creation_commits_safe_mode_reconciliation_provenance() {
    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        for (const marker of [`async function ${name}(`, `function ${name}(`]) {
          const start = source.indexOf(marker);
          if (start < 0) continue;
          const brace = source.indexOf("{", start);
          let depth = 0, quote = "", escaped = false;
          for (let index = brace; index < source.length; index += 1) {
            const character = source[index];
            if (quote) {
              if (escaped) escaped = false;
              else if (character === "\\") escaped = true;
              else if (character === quote) quote = "";
            } else if (["\"", "'", "`"].includes(character)) quote = character;
            else if (character === "{") depth += 1;
            else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
          }
        }
        throw new Error(`missing ${name}`);
      }
      function deferred() {
        let resolve;
        const promise = new Promise((yes) => { resolve = yes; });
        return { promise, resolve };
      }
      const source = fs.readFileSync("extension/background.js", "utf8");
      const functions = [
        "rememberCanceledCommand", "commandCanceledError", "assertCommandActive",
        "withCommandCancellation", "cancelCommandContext", "outcomeUnknownError",
        "assertCommandActiveAfterDispatch", "commandSideEffect",
        "groupBridgeCreatedTab", "appendCommandCleanup", "createBridgeTab",
        "effectiveTabUrl", "isTrackedBridgeBlank",
      ].map((name) => extractFunction(source, name)).join("\n");
      const bridge = new Function("deferred", `
        const canceledCommandKeys = new Set();
        const bridgeCreatedTabs = new Set();
        const creation = deferred();
        const persistence = deferred();
        let persistenceStarted = false;
        let durable = false;
        let groups = 0;
        function initializeControlState() { return Promise.resolve(); }
        function persistControlState() {
          persistenceStarted = true;
          return persistence.promise.then(() => { durable = true; });
        }
        function assertHumanControlAvailable() {}
        function assertHumanControlAvailableAfterDispatch() {}
        const chrome = {
          tabs: {
            create() { return creation.promise; },
            group() { groups += 1; return Promise.resolve(9); },
            remove() { return Promise.resolve(); },
          },
          tabGroups: { update() { return Promise.resolve(); } },
        };
        ${functions}
        return {
          create(context) { return createBridgeTab(context); },
          cancel(context) { cancelCommandContext(context, "request_canceled"); },
          resolveCreation() { creation.resolve({ id: 42, url: "about:blank", active: true }); },
          releasePersistence() { persistence.resolve(); },
          tracked() {
            const tab = { id: 42, url: "about:blank", active: true };
            return isTrackedBridgeBlank(tab) && bridgeCreatedTabs.has(42);
          },
          state() { return { persistenceStarted, durable, groups }; },
        };
      `)(deferred);
      const context = {
        key: "session:new", method: "tabs.new", started: true,
        canceled: false, cancelWaiters: new Set(), cancellationCleanup: Promise.resolve(false),
      };
      const action = bridge.create(context);
      bridge.cancel(context);
      let outcomeUnknown = false;
      try { await action; } catch (error) { outcomeUnknown = error.code === "ACTION_OUTCOME_UNKNOWN"; }
      if (!outcomeUnknown) throw new Error("canceled tab creation was not outcome-unknown");
      bridge.resolveCreation();
      while (!bridge.state().persistenceStarted) await Promise.resolve();
      let nextQueued = false;
      const next = context.cancellationCleanup.then(() => { nextQueued = true; });
      await Promise.resolve();
      if (nextQueued || !bridge.tracked() || bridge.state().durable) {
        throw new Error("tab provenance did not remain queue-blocked until durable persistence");
      }
      bridge.releasePersistence();
      await context.cancellationCleanup;
      await next;
      if (!nextQueued || !bridge.tracked() || !bridge.state().durable || bridge.state().groups !== 1) {
        throw new Error(`canceled tab was not safely reconcilable: ${JSON.stringify(bridge.state())}`);
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run canceled-tab provenance harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Canceled-tab provenance harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn control_state_writes_cannot_resurrect_an_older_turn() {
    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `async function ${name}(`;
        const start = source.indexOf(marker);
        if (start < 0) throw new Error(`missing ${name}`);
        const brace = source.indexOf("{", start);
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      function deferred() {
        let resolve;
        const promise = new Promise((yes) => { resolve = yes; });
        return { promise, resolve };
      }
      const source = fs.readFileSync("extension/background.js", "utf8");
      const persist = extractFunction(source, "persistControlState");
      const bridge = new Function("deferred", `
        const CONTROL_STORAGE_KEY = "lease";
        const CONTROL_REVOCATION_KEY = "revocation";
        const CONTROL_CLEANUPS_KEY = "cleanups";
        const CONTROL_INPUTS_KEY = "inputs";
        const CREATED_TABS_KEY = "tabs";
        let controlLease = { sessionId: "lease", epoch: 2, tabId: 2, turn: 4 };
        let lastControlRevocation = null;
        const pendingControlCleanups = new Map();
        const heldMouseInputs = new Map();
        const heldKeyInputs = new Map();
        const bridgeCreatedTabs = new Set();
        let controlStateWrite = Promise.resolve();
        const calls = [];
        const gates = [];
        let durable = null;
        const chrome = { storage: { session: { set(value) {
          const gate = deferred();
          calls.push(structuredClone(value));
          gates.push(gate);
          return gate.promise.then(() => { durable = structuredClone(value); });
        } } } };
        ${persist}
        return {
          persist: () => persistControlState(),
          setTurn(turn) { controlLease.turn = turn; },
          release(index) { gates[index].resolve(); },
          state() { return { calls: structuredClone(calls), durable: structuredClone(durable) }; },
        };
      `)(deferred);

      const oldWrite = bridge.persist();
      while (bridge.state().calls.length === 0) await Promise.resolve();
      bridge.setTurn(5);
      const newWrite = bridge.persist();
      await Promise.resolve();
      if (bridge.state().calls.length !== 1) {
        throw new Error("the newer durable write ran before the older write settled");
      }
      bridge.release(0);
      await oldWrite;
      while (bridge.state().calls.length < 2) await Promise.resolve();
      if (bridge.state().calls[1].lease.turn !== 5) {
        throw new Error("the serialized write did not capture the canceled-command turn");
      }
      bridge.release(1);
      await newWrite;
      if (bridge.state().durable.lease.turn !== 5) {
        throw new Error("an older write resurrected the previous turn");
      }
    "#;

    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run control-state persistence harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Control-state persistence harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn renderer_and_debugger_lifecycle_waits_are_bounded_and_cancel_aware() {
    let background = extension_source("background.js");
    let content = extension_source("content.js");

    let bounded_start = background
        .find("async function boundedContentOperation")
        .unwrap();
    let bounded_end = background[bounded_start..]
        .find("async function contentRequest")
        .unwrap()
        + bounded_start;
    let bounded = &background[bounded_start..bounded_end];
    assert!(bounded.contains("withCommandCancellation"));
    assert!(bounded.contains("CONTENT_TIMEOUT_MS"));
    assert!(bounded.contains("CONTENT_TIMEOUT"));
    assert!(bounded.contains("COMMAND_CANCELED"));
    assert!(bounded.contains("stopControl"));
    assert!(bounded.contains("outcomeUnknownError"));

    let request_start = background.find("async function contentRequest").unwrap();
    let request_end = background[request_start..].find("function clamp").unwrap() + request_start;
    let request = &background[request_start..request_end];
    assert!(request.matches("boundedContentOperation(").count() >= 3);
    assert!(request.contains("firstError.code === \"ACTION_OUTCOME_UNKNOWN\""));
    assert!(request.contains("secondError.code === \"ACTION_OUTCOME_UNKNOWN\""));

    let attach_start = background
        .find("const attachOperation = chrome.debugger.attach")
        .unwrap();
    let attach_end = background[attach_start..]
        .find("const now = Date.now()")
        .unwrap()
        + attach_start;
    let attach = &background[attach_start..attach_end];
    assert!(attach.contains("withCommandCancellation(attachOperation"));
    assert!(attach.contains("DEBUGGER_LIFECYCLE_TIMEOUT_MS"));
    assert!(attach.contains("DEBUGGER_ATTACH_OUTCOME_UNKNOWN"));
    assert!(attach.contains("late_debugger_attach_resolved"));
    assert!(background.contains("pendingDebuggerAttaches"));
    assert!(background.contains("pendingDebuggerDetaches"));
    assert!(background.contains("DEBUGGER_RECOVERY_TIMEOUT"));

    assert!(content.contains("requestedSessionId !== activeControlSessionId"));
    assert!(content.contains("requestedEpoch !== activeControlEpoch"));
    assert!(content.contains("ignoredStaleSession: true"));
}

#[test]
fn deferred_renderer_delivery_is_canceled_before_late_mutation() {
    let script = r#"
      import fs from "node:fs";

      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (start < 0) throw new Error(`missing ${name}`);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const brace = source.indexOf("{", start);
        let depth = 0;
        let quote = "";
        let escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
            continue;
          }
          if (character === "\"" || character === "'" || character === "`") {
            quote = character;
          } else if (character === "{") {
            depth += 1;
          } else if (character === "}") {
            depth -= 1;
            if (depth === 0) return source.slice(start, index + 1);
          }
        }
        throw new Error(`unterminated ${name}`);
      }

      const background = fs.readFileSync("extension/background.js", "utf8");
      const backgroundFunctions = [
        "rememberCanceledCommand",
        "commandCanceledError",
        "assertCommandActive",
        "withCommandCancellation",
        "cancelCommandContext",
      ].map((name) => extractFunction(background, name)).join("\n");
      const bridge = new Function(`
        const canceledCommandKeys = new Set();
        ${backgroundFunctions}
        return { withCommandCancellation, cancelCommandContext };
      `)();

      const contentSource = fs.readFileSync("extension/content.js", "utf8");
      const activeGuard = extractFunction(contentSource, "assertActiveControl");
      const content = new Function(`
        let activeControlSessionId = "server-session";
        let activeControlEpoch = 7;
        ${activeGuard}
        return {
          assertActiveControl,
          revoke() { activeControlSessionId = ""; activeControlEpoch = 0; },
        };
      `)();

      const context = {
        key: "server-session:4:command-id",
        canceled: false,
        cancelWaiters: new Set(),
      };
      let deliver;
      let mutations = 0;
      const deferredDelivery = new Promise((resolve) => { deliver = resolve; });
      const lateHandler = deferredDelivery.then((message) => {
        content.assertActiveControl(message.controlSessionId, message.controlEpoch);
        mutations += 1;
      });
      const commandWait = bridge.withCommandCancellation(lateHandler, context, "deferred renderer");

      bridge.cancelCommandContext(context, "server_timeout");
      let canceled = false;
      try {
        await commandWait;
      } catch (error) {
        canceled = error.code === "COMMAND_CANCELED";
      }
      if (!canceled) throw new Error("cancel did not release the active command wait");

      content.revoke();
      deliver({ controlSessionId: "server-session", controlEpoch: 7 });
      let rejectedLateDelivery = false;
      try {
        await lateHandler;
      } catch (error) {
        rejectedLateDelivery = error.message.startsWith("CONTROL_REVOKED:");
      }
      if (!rejectedLateDelivery || mutations !== 0) {
        throw new Error("late renderer delivery crossed the revoked session/epoch boundary");
      }
      if (context.cancelWaiters.size !== 0) throw new Error("cancel waiter leaked");
    "#;

    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run Node cancellation harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node cancellation harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn lease_epoch_guards_page_actions_and_content_authority() {
    let background = extension_source("background.js");
    let content = extension_source("content.js");
    assert!(background.contains("let controlEpoch = 0"));
    assert!(background.contains("controlEpoch += 1"));
    assert!(background.contains("controlLease.epoch !== authority.epoch"));
    assert!(background.contains("controlEpoch !== authority.epoch"));
    assert!(background.contains("async function leaseSideEffect(authority, context"));
    assert!(
        background
            .contains("controlEpoch: controlLease?.tabId === tabId ? controlLease.epoch : null")
    );
    assert!(background.contains(
        "async function dispatch(method, params, approved, commandContext = null, { batched = false } = {})"
    ));

    for method in [
        "page.navigate",
        "page.back",
        "page.forward",
        "page.reload",
        "page.click",
        "page.fill",
        "page.select",
        "page.key",
        "page.scroll",
        "page.clickAt",
        "page.typeText",
        "page.evaluate",
    ] {
        let start = background.find(&format!("case \"{method}\"")).unwrap();
        let end = background[start + 5..]
            .find("\n    case \"")
            .map(|offset| start + 5 + offset)
            .unwrap_or(background.len());
        let body = &background[start..end];
        assert!(
            body.contains("commandContext"),
            "{method} is not cancel-bound"
        );
        assert!(
            body.contains("captureLeaseAuthority") || body.contains("leaseSideEffect"),
            "{method} is not lease-epoch-bound"
        );
    }

    assert!(content.contains("let activeControlEpoch = 0"));
    assert!(content.contains("requestedEpoch !== activeControlEpoch"));
    assert!(content.contains("message.controlEpoch ?? message.epoch"));
    assert!(
        content.contains("assertActiveControl(message.controlSessionId, message.controlEpoch)")
    );
}

#[test]
fn cdp_timeout_and_held_input_cleanup_fail_closed() {
    let background = extension_source("background.js");
    let debugger_start = background.find("async function debuggerCommand").unwrap();
    let debugger_end = background[debugger_start..]
        .find("const POINTER_CANDIDATE_COUNT")
        .unwrap()
        + debugger_start;
    let debugger = &background[debugger_start..debugger_end];
    assert!(debugger.contains("operation.catch(() => {})"));
    assert!(debugger.contains("error.code === \"DEBUGGER_TIMEOUT\""));
    assert!(debugger.contains("stopControl(`cdp_timeout:${method}`"));
    assert!(debugger.contains("CDP_OUTCOME_UNKNOWN"));
    assert!(debugger.contains("automatic retry is unsafe"));

    let key_start = background.find("async function trustedKey").unwrap();
    let key_end = background[key_start..]
        .find("async function insertText")
        .unwrap()
        + key_start;
    let key = &background[key_start..key_end];
    assert!(key.contains("persistHeldInputIntent(heldKeyInputs"));
    assert!(key.contains("type: \"rawKeyDown\""));
    assert!(key.contains("type: \"keyUp\""));
    assert!(key.contains("finally"));
    assert!(key.contains("releaseHeldKeyInput"));

    for function in [
        "async function trustedClick(",
        "async function trustedClickAt(",
    ] {
        let start = background.find(function).unwrap();
        let end = background[start..].find("\n}\n").unwrap() + start + 3;
        let body = &background[start..end];
        assert!(body.contains("persistHeldInputIntent(heldMouseInputs"));
        assert!(body.contains("finally"));
        assert!(body.contains("releaseHeldMouseInput"));
    }
    assert!(background.contains("INPUT_RELEASE_FAILED"));
    assert!(background.contains("releaseHeldInputs(lease.tabId)"));
}

#[test]
fn failed_release_or_detach_stays_cleanup_pending_until_verified() {
    let background = extension_source("background.js");
    let popup = extension_source("popup.js");
    let stop_start = background.find("async function stopControl").unwrap();
    let stop_end = background[stop_start..]
        .find("async function confirmPendingControlCleanup")
        .unwrap()
        + stop_start;
    let stop = &background[stop_start..stop_end];
    assert!(stop.contains("inputsReleased = await releaseHeldInputs"));
    assert!(stop.contains("detachConfirmed = await debuggerDetachConfirmed"));
    assert!(stop.contains("!pendingDebuggerDetaches.has"));
    assert!(stop.contains("pendingControlCleanups.set"));
    assert!(stop.contains("cleanupPending: true"));
    assert!(stop.find("if (detachConfirmed)").unwrap() < stop.find("forgetHeldInputs").unwrap());

    assert!(background.contains("async function retryPendingControlCleanups"));
    assert!(background.contains("browser.control.cleanup_completed"));
    assert!(background.contains("revocationPending: Boolean(cleanup)"));
    assert!(background.contains("if (pendingControlCleanups.size)"));
    assert!(popup.contains("Release cleanup pending"));
    assert!(popup.contains("control.revocationPending"));
}

#[test]
fn cleanup_on_tab_a_blocks_tab_b_and_survives_worker_restart() {
    let background = extension_source("background.js");
    assert!(background.contains("const CONTROL_CLEANUPS_KEY = \"browserControlCleanups\""));
    assert!(background.contains("[CONTROL_CLEANUPS_KEY]: [...pendingControlCleanups.values()]"));
    assert!(background.contains("[CONTROL_CLEANUPS_KEY]: []"));
    assert!(background.contains(
        "for (const cleanup of Array.isArray(stored[CONTROL_CLEANUPS_KEY]) ? stored[CONTROL_CLEANUPS_KEY] : [])"
    ));

    let guard_start = background
        .find("function assertNoPendingControlLifecycle")
        .unwrap();
    let guard_end = background[guard_start..].find("\n}\n").unwrap() + guard_start + 3;
    let guard = &background[guard_start..guard_end];
    assert!(guard.contains("pendingControlCleanups.size > 0"));
    assert!(guard.contains("pendingControlTeardowns.size > 0"));
    assert!(!guard.contains(".has(tabId)"));
    assert!(
        background
            .matches("assertNoPendingControlLifecycle();")
            .count()
            >= 2
    );

    let stop_start = background.find("async function stopControl").unwrap();
    let stop_end = background[stop_start..]
        .find("async function confirmPendingControlCleanup")
        .unwrap()
        + stop_start;
    let stop = &background[stop_start..stop_end];
    assert!(
        stop.find("pendingControlCleanups.set(lease.tabId, cleanup)")
            .unwrap()
            < stop.find("persistControlState().catch").unwrap()
    );
    assert!(
        stop.find("persistControlState().catch").unwrap()
            < stop.find("releaseHeldInputs(lease.tabId)").unwrap()
    );

    let public_start = background.find("function publicControlState").unwrap();
    let public_end = background[public_start..]
        .find("async function persistControlState")
        .unwrap()
        + public_start;
    let public_state = &background[public_start..public_end];
    assert!(
        public_state
            .matches("revocationPending: Boolean(cleanup)")
            .count()
            >= 2
    );
    assert!(public_state.matches("cleanups,").count() >= 2);
}

#[test]
fn late_attach_detach_failure_remains_durable_and_globally_quarantined() {
    let background = extension_source("background.js");
    let start = background.find("async function startControl").unwrap();
    let end = background[start..]
        .find("async function requireControl")
        .unwrap()
        + start;
    let start_control = &background[start..end];
    let intent = start_control
        .find("reason: \"debugger_attach_intent\"")
        .unwrap();
    let durable_write = start_control[intent..]
        .find("await persistControlState()")
        .unwrap()
        + intent;
    let attach_dispatch = start_control.find("chrome.debugger.attach").unwrap();
    assert!(intent < durable_write && durable_write < attach_dispatch);
    assert!(start_control.contains("phase: \"attach_intent\""));
    assert!(start_control.contains("markUnknownAttachCleanup"));
    assert!(start_control.contains("settleUnknownDebuggerAttach"));
    assert!(start_control.contains("late_debugger_attach_resolved"));
    assert!(start_control.contains("late_debugger_attach_rejected"));
    assert!(
        !start_control.contains(
            "await boundedDebuggerDetach(tab.id, \"canceled debugger attachment cleanup\")"
        )
    );

    let settle_start = background
        .find("async function settleUnknownDebuggerAttach")
        .unwrap();
    let settle_end = background[settle_start..]
        .find("async function hardRevokeDetached")
        .unwrap()
        + settle_start;
    let settle = &background[settle_start..settle_end];
    assert!(settle.contains("markUnknownAttachCleanup"));
    assert!(settle.contains("retryPendingControlCleanups"));
    assert!(settle.contains("return !pendingControlCleanups.has(tabId)"));

    let retry_start = background
        .find("async function retryPendingControlCleanups")
        .unwrap();
    let retry_end = background[retry_start..]
        .find("async function markUnknownAttachCleanup")
        .unwrap()
        + retry_start;
    let retry = &background[retry_start..retry_end];
    assert!(retry.contains("pendingDebuggerAttaches.has(tabId)"));
    assert!(retry.contains("debuggerDetachConfirmed(tabId) === true"));
    assert!(retry.contains("!pendingDebuggerDetaches.has(tabId)"));
    assert!(retry.contains("pendingControlCleanups.set(tabId"));

    assert!(background.contains("[CONTROL_CLEANUPS_KEY]: [...pendingControlCleanups.values()]"));
    assert!(background.contains("pendingControlCleanups.size > 0"));
}

#[test]
fn human_stop_globally_pauses_remote_control_until_trusted_popup_resume() {
    let background = extension_source("background.js");
    let popup = extension_source("popup.js");
    assert!(background.contains("const HUMAN_CONTROL_PAUSE_KEY = \"browserControlHumanPause\""));
    assert!(background.contains("new Set([\"released_by_user\", \"canceled_by_user\"])"));
    assert!(background.contains("chrome.storage.local.get({ [HUMAN_CONTROL_PAUSE_KEY]: null })"));
    assert!(
        background
            .contains("chrome.storage.local.set({ [HUMAN_CONTROL_PAUSE_KEY]: humanControlPause })")
    );

    let stop_start = background.find("async function stopControl").unwrap();
    let stop_end = background[stop_start..]
        .find("async function confirmPendingControlCleanup")
        .unwrap()
        + stop_start;
    let stop = &background[stop_start..stop_end];
    assert!(
        stop.find("latchHumanControlPause(reason, controlLease)")
            .unwrap()
            < stop.find("synchronouslyTakeControlLease").unwrap()
    );
    assert!(stop.contains("persistHumanControlPause"));
    let hard_start = background
        .find("async function hardRevokeDetached")
        .unwrap();
    let hard_end = background[hard_start..]
        .find("async function heartbeatControl")
        .unwrap()
        + hard_start;
    let hard = &background[hard_start..hard_end];
    assert!(hard.contains("pausePersistError"));
    assert!(hard.contains("noteHumanPausePersistenceFailure"));
    assert!(!hard.contains("persistHumanControlPause().catch(() => {})"));

    let dispatch_start = background.find("async function dispatch").unwrap();
    let dispatch_end = background[dispatch_start..]
        .find("async function popupState")
        .unwrap()
        + dispatch_start;
    let dispatch = &background[dispatch_start..dispatch_end];
    assert!(dispatch.contains("if (!PAUSE_ALLOWED_COMMANDS.has(method))"));
    for allowed in [
        "status",
        "tabs.list",
        "browser.control.status",
        "browser.control.stop",
    ] {
        assert!(background.contains(&format!("\"{allowed}\"")));
    }
    for blocked in ["tabs.activate", "tabs.new", "tabs.close"] {
        assert!(
            !background
                .split("const PAUSE_ALLOWED_COMMANDS = new Set([")
                .nth(1)
                .unwrap()
                .split("]);")
                .next()
                .unwrap()
                .contains(&format!("\"{blocked}\""))
        );
    }
    assert!(dispatch.contains("assertHumanControlAvailable()"));
    assert!(background.contains("async function requireControl"));
    assert!(background.matches("assertHumanControlAvailable();").count() >= 3);

    let resume_start = background.find("case \"resumeRemoteControl\"").unwrap();
    let resume_end = background[resume_start..]
        .find("case \"releaseControl\"")
        .unwrap()
        + resume_start;
    let resume = &background[resume_start..resume_end];
    assert!(resume.contains("sender.url !== chrome.runtime.getURL(\"popup.html\")"));
    assert!(resume.contains("resumeHumanControlFromPopup"));
    assert!(!background.contains("\"resumeRemoteControl\","));

    assert!(popup.contains("Resume remote control"));
    assert!(popup.contains("resumeRemoteControl"));
    assert!(popup.contains("All remote browser control is paused"));
    assert!(
        background
            .matches("humanPaused: Boolean(humanControlPause?.paused)")
            .count()
            >= 2
    );
}

#[test]
fn human_pause_behavior_survives_restart_and_resume_reauthorizes() {
    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (start < 0) throw new Error(`missing ${name}`);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const brace = source.indexOf("{", start);
        let depth = 0;
        let quote = "";
        let escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
            continue;
          }
          if (character === "\"" || character === "'" || character === "`") quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }

      const source = fs.readFileSync("extension/background.js", "utf8");
      const functions = [
        "humanControlPausedError",
        "assertHumanControlAvailable",
        "latchHumanControlPause",
        "persistHumanControlPause",
        "resumeHumanControlFromPopup",
      ].map((name) => extractFunction(source, name)).join("\n");
      const makeWorker = new Function("initialPause", "initialUncertain", "durable", `
        const HUMAN_CONTROL_PAUSE_KEY = "browserControlHumanPause";
        const HUMAN_CONTROL_PAUSE_UNCERTAIN_KEY = "browserControlHumanPauseUncertain";
        const HUMAN_PAUSE_REASONS = new Set(["released_by_user", "canceled_by_user"]);
        let humanControlPause = initialPause;
        let humanControlPauseUncertain = initialUncertain;
        let controlLease = null;
        const chrome = { storage: {
          local: { set: async (update) => {
            if (durable.failLocal) throw new Error("local storage rejected");
            durable.local = structuredClone(update[HUMAN_CONTROL_PAUSE_KEY]);
          } },
          session: { set: async (update) => {
            if (durable.failSession) throw new Error("session storage rejected");
            durable.session = structuredClone(update[HUMAN_CONTROL_PAUSE_UNCERTAIN_KEY]);
          } },
        } };
        ${functions}
        return {
          stop(reason, lease) { latchHumanControlPause(reason, lease); },
          persistHumanControlPause,
          assertRemote: assertHumanControlAvailable,
          resume: resumeHumanControlFromPopup,
        };
      `);

      const durable = { local: null, session: null, failLocal: false, failSession: false };
      const firstWorker = makeWorker(null, null, durable);
      firstWorker.stop("released_by_user", { tabId: 11, sessionId: "session-a" });
      await firstWorker.persistHumanControlPause();
      for (const action of [
        "start-a", "start-b", "page-a", "page-b",
        "tabs.activate", "tabs.new", "tabs.close",
      ]) {
        let rejected = false;
        try { firstWorker.assertRemote(); }
        catch (error) { rejected = error.code === "HUMAN_CONTROL_PAUSED"; }
        if (!rejected) throw new Error(`${action} bypassed the human pause`);
      }

      const restartedWorker = makeWorker(structuredClone(durable.local), structuredClone(durable.session), durable);
      let restartRejected = false;
      try { restartedWorker.assertRemote(); }
      catch (error) { restartRejected = error.code === "HUMAN_CONTROL_PAUSED"; }
      if (!restartRejected) throw new Error("worker restart lost the human pause");

      await restartedWorker.resume();
      restartedWorker.assertRemote();
      if (durable.local !== null || durable.session !== null) {
        throw new Error("trusted resume did not durably clear pause");
      }

      const rejectingDurable = { local: null, session: null, failLocal: true, failSession: false };
      const rejectingWorker = makeWorker(null, null, rejectingDurable);
      rejectingWorker.stop("released_by_user", { tabId: 22, sessionId: "session-b" });
      let persistRejected = false;
      try { await rejectingWorker.persistHumanControlPause(); }
      catch (error) { persistRejected = error.code === "HUMAN_PAUSE_PERSIST_FAILED"; }
      if (!persistRejected || !rejectingDurable.session?.paused) {
        throw new Error("pause storage rejection did not leave a durable uncertain marker");
      }
      let liveWorkerRejected = false;
      try { rejectingWorker.assertRemote(); }
      catch (error) { liveWorkerRejected = error.code === "HUMAN_CONTROL_PAUSED"; }
      if (!liveWorkerRejected) throw new Error("pause storage rejection failed open in the live worker");

      const uncertainRestart = makeWorker(
        structuredClone(rejectingDurable.session.pause),
        structuredClone(rejectingDurable.session),
        rejectingDurable,
      );
      let uncertainRestartRejected = false;
      try { uncertainRestart.assertRemote(); }
      catch (error) { uncertainRestartRejected = error.code === "HUMAN_CONTROL_PAUSED"; }
      if (!uncertainRestartRejected) throw new Error("uncertain pause failed open after worker restart");

      let resumeRejected = false;
      try { await uncertainRestart.resume(); }
      catch { resumeRejected = true; }
      if (!resumeRejected) throw new Error("rejected resume storage write was acknowledged");
      let afterResumeFailureRejected = false;
      try { uncertainRestart.assertRemote(); }
      catch (error) { afterResumeFailureRejected = error.code === "HUMAN_CONTROL_PAUSED"; }
      if (!afterResumeFailureRejected) throw new Error("resume storage rejection cleared the live pause");

      const sessionRejectingDurable = {
        local: null, session: null, failLocal: false, failSession: true,
      };
      const sessionRejectingWorker = makeWorker(null, null, sessionRejectingDurable);
      sessionRejectingWorker.stop("released_by_user", { tabId: 33, sessionId: "session-c" });
      await sessionRejectingWorker.persistHumanControlPause();
      if (!sessionRejectingDurable.local?.paused) {
        throw new Error("session-marker failure prevented independent durable local pause");
      }
      const localBackedRestart = makeWorker(
        structuredClone(sessionRejectingDurable.local),
        null,
        sessionRejectingDurable,
      );
      let localBackedRestartRejected = false;
      try { localBackedRestart.assertRemote(); }
      catch (error) { localBackedRestartRejected = error.code === "HUMAN_CONTROL_PAUSED"; }
      if (!localBackedRestartRejected) {
        throw new Error("local pause failed open after session-marker failure and worker restart");
      }

      const cancelDurable = { local: null, session: null, failLocal: true, failSession: false };
      const canceledWorker = makeWorker(null, null, cancelDurable);
      canceledWorker.stop("canceled_by_user", { tabId: 44, sessionId: "session-d" });
      try { await canceledWorker.persistHumanControlPause(); } catch {}
      const canceledRestart = makeWorker(
        structuredClone(cancelDurable.session.pause),
        structuredClone(cancelDurable.session),
        cancelDurable,
      );
      let canceledRestartRejected = false;
      try { canceledRestart.assertRemote(); }
      catch (error) { canceledRestartRejected = error.code === "HUMAN_CONTROL_PAUSED"; }
      if (!canceledRestartRejected) {
        throw new Error("Chrome canceled_by_user failed open after persistence rejection and restart");
      }
    "#;

    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run Node human-pause harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node human-pause harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn human_stop_wins_deferred_tab_mutations_and_debugger_attach() {
    let background = extension_source("background.js");
    let side_effect_start = background.find("async function commandSideEffect").unwrap();
    let side_effect_end = background[side_effect_start..]
        .find("async function leaseSideEffect")
        .unwrap()
        + side_effect_start;
    let side_effect = &background[side_effect_start..side_effect_end];
    assert!(
        side_effect.find("assertHumanControlAvailable();").unwrap()
            < side_effect.find("operation()").unwrap()
    );
    assert!(
        side_effect.find("operation()").unwrap()
            < side_effect
                .find("assertHumanControlAvailableAfterDispatch(boundary)")
                .unwrap()
    );

    for (method, chrome_call) in [
        ("tabs.activate", "chrome.tabs.update"),
        ("tabs.close", "chrome.tabs.remove"),
    ] {
        let start = background.find(&format!("case \"{method}\"")).unwrap();
        let end = background[start + 5..]
            .find("\n    case \"")
            .map(|offset| start + 5 + offset)
            .unwrap_or(background.len());
        let body = &background[start..end];
        assert!(body.contains("commandSideEffect"));
        assert!(body.contains(chrome_call));
    }
    let new_start = background.find("case \"tabs.new\"").unwrap();
    let new_end = background[new_start + 5..]
        .find("\n    case \"")
        .map(|offset| new_start + 5 + offset)
        .unwrap_or(background.len());
    assert!(
        background[new_start..new_end]
            .contains("createAllowedBridgeTab(params.url, commandContext)")
    );
    let validation_start = background
        .find("async function createAllowedBridgeTab")
        .unwrap();
    let validation_end = background[validation_start..]
        .find("async function createBridgeTab")
        .unwrap()
        + validation_start;
    let validation = &background[validation_start..validation_end];
    assert!(validation.contains("createBridgeTab(commandContext, creationUrl)"));
    let create_start = background.find("async function createBridgeTab").unwrap();
    let create_end = background[create_start..]
        .find("async function clearPendingDialog")
        .unwrap()
        + create_start;
    let create = &background[create_start..create_end];
    assert!(create.contains("chrome.tabs.create"));
    assert!(create.contains("assertHumanControlAvailable();"));
    assert!(create.contains("assertHumanControlAvailableAfterDispatch"));
    assert!(create.contains("appendCommandCleanup(commandContext, provenance)"));
    let group_start = background
        .find("async function groupBridgeCreatedTab")
        .unwrap();
    let group_end = background[group_start..]
        .find("function appendCommandCleanup")
        .unwrap()
        + group_start;
    let group = &background[group_start..group_end];
    assert!(
        group.find("bridgeCreatedTabs.add(tab.id)").unwrap()
            < group.find("commandSideEffect").unwrap()
    );
    let approve_start = background.find("case \"approve\"").unwrap();
    let approve_end = background[approve_start..].find("case \"reject\"").unwrap() + approve_start;
    assert!(
        background[approve_start..approve_end]
            .contains("dispatch(pending.method, pending.params, true)")
    );

    let start_control_start = background.find("async function startControl").unwrap();
    let start_control_end = background[start_control_start..]
        .find("async function requireControl")
        .unwrap()
        + start_control_start;
    let start_control = &background[start_control_start..start_control_end];
    let attach_intent = start_control
        .find("reason: \"debugger_attach_intent\"")
        .unwrap();
    let attach_dispatch = start_control.find("chrome.debugger.attach").unwrap();
    assert!(start_control[..attach_intent].contains("assertHumanControlAvailable();"));
    assert!(
        start_control[..attach_dispatch]
            .rfind("assertHumanControlAvailable();")
            .unwrap()
            < attach_dispatch
    );
    assert!(start_control[attach_dispatch..].contains("human_pause_after_debugger_attach"));
    assert!(start_control[attach_dispatch..].contains("throw outcomeUnknownError"));

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const asyncMarker = `async function ${name}(`;
        const plainMarker = `function ${name}(`;
        const start = source.includes(asyncMarker)
          ? source.indexOf(asyncMarker)
          : source.indexOf(plainMarker);
        const signatureEnd = source.indexOf(") {", start);
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      function deferred() {
        let resolve, reject;
        const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
        return { promise, resolve, reject };
      }
      const source = fs.readFileSync("extension/background.js", "utf8");
      const sideEffectFunctions = [
        "humanControlPausedError", "assertHumanControlAvailable", "outcomeUnknownError",
        "assertCommandActive", "assertCommandActiveAfterDispatch",
        "assertHumanControlAvailableAfterDispatch", "withCommandCancellation",
        "commandSideEffect",
      ].map((name) => extractFunction(source, name)).join("\n");
      const mutationBridge = new Function("deferred", `
        let humanControlPause = null;
        let humanControlPauseUncertain = null;
        const canceledCommandKeys = new Set();
        ${sideEffectFunctions}
        return {
          run(boundary, operation) { return commandSideEffect(null, boundary, operation); },
          pause() { humanControlPause = { paused: true, reason: "released_by_user" }; },
          resume() { humanControlPause = null; humanControlPauseUncertain = null; },
        };
      `)(deferred);

      for (const boundary of ["tab activation", "tab creation", "tab close", "approved tab close"]) {
        mutationBridge.resume();
        const operation = deferred();
        let dispatched = 0;
        const pending = mutationBridge.run(boundary, () => {
          dispatched += 1;
          return operation.promise;
        });
        mutationBridge.pause();
        operation.resolve({ ok: true });
        let outcomeUnknown = false;
        try { await pending; }
        catch (error) { outcomeUnknown = error.code === "ACTION_OUTCOME_UNKNOWN"; }
        if (dispatched !== 1 || !outcomeUnknown) {
          throw new Error(`${boundary} did not report unknown outcome when Stop won after dispatch`);
        }

        let lateDispatches = 0;
        try { await mutationBridge.run(boundary, async () => { lateDispatches += 1; }); }
        catch (error) {
          if (error.code !== "HUMAN_CONTROL_PAUSED") throw error;
        }
        if (lateDispatches !== 0) throw new Error(`${boundary} crossed a pre-dispatch human pause`);
      }

      const startFunctions = [
        "humanControlPausedError", "assertHumanControlAvailable", "outcomeUnknownError",
        "startControl", "settleUnknownDebuggerAttach",
      ].map((name) => extractFunction(source, name)).join("\n");
      const startBridge = new Function("deferred", `
        const CONTROL_TTL_DEFAULT_MS = 60_000;
        const CONTROL_TTL_MIN_MS = 1_000;
        const CONTROL_TTL_MAX_MS = 120_000;
        const DEBUGGER_LIFECYCLE_TIMEOUT_MS = 3_000;
        let humanControlPause = null;
        let humanControlPauseUncertain = null;
        let controlEpoch = 1;
        let controlLease = { tabId: 1, sessionId: "session-a", ownerSessionId: "owner" };
        let lastControlRevocation = null;
        const pendingControlCleanups = new Map();
        const pendingDebuggerAttaches = new Map();
        const canceledCommandKeys = new Set();
        let tabGate = deferred();
        let attachGate = deferred();
        let tabCalls = 0, attachCalls = 0, detachAttempts = 0, durableWrites = 0;
        const chrome = { debugger: { attach() { attachCalls += 1; return attachGate.promise; } } };
        function initializeControlState() { return Promise.resolve(); }
        function initializeProtocolIdentity() { return Promise.resolve(); }
        function getTab() { tabCalls += 1; return tabGate.promise; }
        function assertAllowedTab() { return Promise.resolve(); }
        function settings() { return Promise.resolve({ allowedHosts: [], port: 17373, fullAccess: true }); }
        function assertNoPendingControlLifecycle() {}
        function currentControlOwner() { return "owner"; }
        function clamp(value, minimum, maximum) { return Math.min(maximum, Math.max(minimum, value)); }
        function assertCommandActive() {}
        function persistControlState() { durableWrites += 1; return Promise.resolve(); }
        function stopControl() { controlEpoch += 1; controlLease = null; return Promise.resolve(); }
        function withCommandCancellation(promise) { return promise; }
        function withTimeout(promise) { return promise; }
        function markUnknownAttachCleanup(tabId, attachToken, reason, extra = {}) {
          pendingControlCleanups.set(tabId, {
            tabId, attachToken, reason, phase: "attach_outcome_unknown", ...extra,
          });
          durableWrites += 1;
          return Promise.resolve();
        }
        function retryPendingControlCleanups() { detachAttempts += 1; return Promise.resolve(); }
        function commandCanceledError() { return new Error("canceled"); }
        ${startFunctions}
        return {
          startB: () => startControl(2, { explicit: true, ownerSessionId: "owner" }),
          reachTab() { return tabCalls; },
          releaseTab() { tabGate.resolve({ id: 2, url: "https://allowed.example/", windowId: 1 }); },
          reachAttach() { return attachCalls; },
          releaseAttach() { attachGate.resolve(); },
          userStop() {
            humanControlPause = { paused: true, reason: "released_by_user" };
            controlEpoch += 1;
            controlLease = null;
          },
          resetForAttachRace() {
            humanControlPause = null;
            controlLease = null;
            lastControlRevocation = null;
            pendingControlCleanups.clear();
            pendingDebuggerAttaches.clear();
            tabGate = { promise: Promise.resolve({ id: 2, url: "https://allowed.example/", windowId: 1 }) };
            attachGate = deferred();
            tabCalls = 0; attachCalls = 0; detachAttempts = 0; durableWrites = 0;
          },
          state: () => ({
            attachCalls, detachAttempts, durableWrites,
            cleanup: pendingControlCleanups.get(2) || null,
          }),
        };
      `)(deferred);

      const beforeAttach = startBridge.startB();
      while (startBridge.reachTab() === 0) await Promise.resolve();
      startBridge.userStop();
      startBridge.releaseTab();
      let preAttachPaused = false;
      try { await beforeAttach; }
      catch (error) { preAttachPaused = error.code === "HUMAN_CONTROL_PAUSED"; }
      if (!preAttachPaused || startBridge.state().attachCalls !== 0) {
        throw new Error("Stop A did not prevent the deferred start B attachment");
      }

      startBridge.resetForAttachRace();
      const afterAttach = startBridge.startB();
      while (startBridge.reachAttach() === 0) await Promise.resolve();
      startBridge.userStop();
      startBridge.releaseAttach();
      let postAttachUnknown = false;
      let postAttachError = null;
      try { await afterAttach; }
      catch (error) {
        postAttachError = { code: error.code, message: error.message };
        postAttachUnknown = error.code === "ACTION_OUTCOME_UNKNOWN";
      }
      const state = startBridge.state();
      if (!postAttachUnknown || state.detachAttempts !== 1
        || state.cleanup?.phase !== "attach_outcome_unknown" || state.durableWrites < 2) {
        throw new Error(`post-dispatch Stop did not leave a durable debugger cleanup quarantine: ${JSON.stringify({ postAttachUnknown, postAttachError, state })}`);
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run deferred human-stop race harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node deferred human-stop race harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn restored_unknown_attach_needs_repeated_detach_confirmation() {
    let background = extension_source("background.js");
    assert!(background.contains("operationSettled: Boolean(cleanup.operationSettled)"));
    assert!(background.contains("detachConfirmations: Math.max(0"));
    assert!(background.contains("unresolvedRestoredAttach"));
    assert!(background.contains("detachConfirmations < 2"));
    assert!(background.contains("now - cleanup.lastDetachConfirmationAt >= 1_000"));
    assert!(background.contains("setTimeout(() => void retryPendingControlCleanups(), 1_000)"));
    assert!(background.contains("{ operationSettled: true }"));
}

#[test]
fn control_ui_paint_capture_and_stop_failures_are_fail_closed() {
    let background = extension_source("background.js");
    let content = extension_source("content.js");

    let show_start = background.find("async function showControlUi").unwrap();
    let show_end = background[show_start..]
        .find("async function hideControlUi")
        .unwrap()
        + show_start;
    let show = &background[show_start..show_end];
    assert!(show.contains("activeCaptureIds"));
    assert!(show.contains("controlUiAcknowledged"));
    assert!(show.contains("failControlUiClosed"));
    assert!(!show.contains(".catch(() => {})"));

    for acknowledgement in [
        "hostConnected",
        "popoverOpen",
        "pillVisible",
        "stopVisible",
        "captureDepth",
        "activeCaptureIds",
    ] {
        assert!(content.contains(acknowledgement));
    }
    assert!(content.contains("await waitForPaint()"));
    assert!(content.contains("Stop failed—use Chrome Cancel or the extension popup."));
    assert!(content.contains("Retry Stop"));

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const brace = source.indexOf("{", start);
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/content.js", "utf8");
      const requestControlStop = extractFunction(source, "requestControlStop");
      const stop = { disabled: false };
      let hidden = false;
      let failureVisible = false;
      const bridge = new Function("stop", `
        let activeControlSessionId = "control-session";
        let controlUi = { stop };
        const chrome = { runtime: { sendMessage: async () => { throw new Error("forced rejection"); } } };
        function hideControl() { hidden = true; }
        let hidden = false;
        function showControlStopFailure() { failureVisible = true; stop.disabled = false; }
        let failureVisible = false;
        ${requestControlStop}
        return {
          request: () => requestControlStop(stop),
          state: () => ({ hidden, failureVisible, disabled: stop.disabled }),
        };
      `)(stop);
      const result = await bridge.request();
      const state = bridge.state();
      if (result.stopped || state.hidden || !state.failureVisible || state.disabled) {
        throw new Error("rejected Stop hid or disabled the live safety indicator");
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run Stop rejection harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node Stop rejection harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn start_then_stop_records_explicit_revocation_without_runtime_reference_errors() {
    let script = r#"
      import fs from "node:fs";
      const source = fs.readFileSync("extension/background.js", "utf8");
      const start = source.indexOf("function synchronouslyTakeControlLease");
      const brace = source.indexOf("{", start);
      let depth = 0, end = -1;
      for (let index = brace; index < source.length; index += 1) {
        if (source[index] === "{") depth += 1;
        else if (source[index] === "}" && --depth === 0) { end = index + 1; break; }
      }
      const fn = source.slice(start, end);
      const bridge = new Function(`
        let controlEpoch = 0;
        let controlLease = null;
        let lastControlRevocation = null;
        const activeControlCaptures = new Map();
        function stopHeartbeat() {}
        function clearFrameSessions() {}
        ${fn}
        return {
          start() {
            controlEpoch += 1;
            controlLease = { tabId: 7, sessionId: "lease-7", epoch: controlEpoch };
          },
          stop() { return synchronouslyTakeControlLease("released_by_user", true); },
          revocation() { return lastControlRevocation; },
        };
      `)();
      bridge.start();
      const lease = bridge.stop();
      const revocation = bridge.revocation();
      if (lease.sessionId !== "lease-7" || revocation.requiresExplicitStart !== true) {
        throw new Error("start -> stop did not record the explicit revocation");
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run start-stop harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node start-stop harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn safe_mode_blank_tabs_require_bridge_provenance() {
    let background = extension_source("background.js");
    let library = extension_source("lib.js");
    assert!(library.contains("fullAccess || trustedBlank"));
    assert!(library.contains("Untracked blank tabs are blocked in Safe mode"));
    assert!(background.contains("bridgeCreatedTabs.has(tab.id)"));
    assert!(background.contains("!Number.isInteger(tab.openerTabId)"));
    assert!(background.contains("tab?.pendingUrl || tab?.url"));
    assert!(background.contains("url: safeUrlForDisplay(effectiveTabUrl(tab))"));
    assert!(
        background
            .contains("allowedTabs = tabs.filter((tab) => allowedTabVerdict(tab, config).allowed)")
    );
    assert!(background.contains("bridge blank provenance recovery"));
    assert!(background.contains("changeInfo.url !== \"about:blank\""));
    assert!(background.contains("bridgeCreatedTabs.delete(tabId)"));

    let script = r#"
      const { isUrlAllowed } = await import("./extension/lib.js");
      const hosts = ["allowed.example"];
      if (isUrlAllowed("about:blank", hosts, 17373, false, false).allowed) {
        throw new Error("untracked inherited blank bypassed Safe mode");
      }
      if (!isUrlAllowed("about:blank", hosts, 17373, false, true).allowed) {
        throw new Error("bridge-created blank was not accepted");
      }
      if (!isUrlAllowed("about:blank", hosts, 17373, true, false).allowed) {
        throw new Error("Full Access did not explicitly accept blank");
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run blank provenance harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node blank provenance harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn document_epoch_revokes_unexpected_navigation_before_capture_or_input() {
    let background = extension_source("background.js");
    assert!(background.contains("chrome.debugger.onEvent.addListener"));
    assert!(background.contains("Page.frameNavigated"));
    assert!(background.contains("Page.navigatedWithinDocument"));
    assert!(background.contains("unexpected_top_level_navigation"));
    assert!(background.contains("navigation_left_allowlist"));
    assert!(background.contains("documentEpoch: lease.documentEpoch"));
    assert!(background.contains("Page.getFrameTree"));
    assert!(background.contains("verifyDocumentAuthority(tab.id, authority"));
    assert!(background.contains("async function verifyDocumentAuthorityAfterDispatch"));
    assert!(background.contains("throw outcomeUnknownError(boundary, error)"));
    for boundary in [
        "text insertion completion",
        "JavaScript evaluation completion",
        "trusted click completion",
        "key completion",
    ] {
        assert!(background.contains(&format!(
            "verifyDocumentAuthorityAfterDispatch(tabId, authority, commandContext, \"{boundary}\")"
        )));
    }

    let capture_start = background.find("async function captureTab").unwrap();
    let capture_end = background[capture_start..]
        .find("async function debuggerCommand")
        .unwrap()
        + capture_start;
    let capture = &background[capture_start..capture_end];
    assert!(capture.matches("verifyDocumentAuthority").count() >= 2);
    assert!(
        capture.find("Page.captureScreenshot").unwrap()
            < capture.rfind("verifyDocumentAuthority").unwrap()
    );

    for function in [
        "async function trustedClick(",
        "async function trustedClickAt(",
    ] {
        let start = background.find(function).unwrap();
        let end = background[start..].find("\n}\n").unwrap() + start + 3;
        let body = &background[start..end];
        assert!(body.matches("verifyDocumentAuthority").count() >= 3);
        assert!(body.find("commit").unwrap() < body.find("mousePressed").unwrap());
    }

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        const start = source.indexOf(marker);
        const signatureEnd = source.indexOf(") {", start);
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/background.js", "utf8");
      const functions = [
        "comparableDocumentUrl",
        "sameDocumentUrl",
        "revokeUnexpectedNavigation",
        "acceptTopLevelNavigationSignal",
      ].map((name) => extractFunction(source, name)).join("\n");
      const bridge = new Function(`
        let controlLease = null;
        const revocations = [];
        function policyUrlVerdict(_lease, url) {
          return { allowed: String(url).startsWith("https://allowed.example/") };
        }
        function stopControl(reason) {
          revocations.push(reason);
          controlLease = null;
          return Promise.resolve();
        }
        function persistControlState() { return Promise.resolve(); }
        const attachedFrames = new Map();
        const frameParents = new Map();
        let frameSnapshot = { generation: "g1" };
        let rootWorldContextId = 11;
        let frameSupport = { supported: true, probed: true, reason: "" };
        const frameInvalidations = [];
        function markFrameTreeChanged(reason) { frameInvalidations.push(reason); }
        ${functions}
        return {
          frameInvalidations: () => [...frameInvalidations],
          frameState: () => ({
            attached: attachedFrames.size,
            snapshot: frameSnapshot,
            rootWorldContextId,
            supported: frameSupport.supported,
          }),
          setLease(pendingNavigation = null) {
            controlLease = {
              tabId: 9, navigationReady: true, pendingNavigation,
              loaderId: "loader-a", frameId: "frame-a",
              documentUrl: "https://allowed.example/a",
              viewport: {}, cursor: { visible: true },
            };
          },
          signal: acceptTopLevelNavigationSignal,
          state: () => ({ lease: controlLease, revocations: [...revocations] }),
        };
      `)();

      bridge.setLease();
      bridge.signal({
        tabId: 9, url: "https://blocked.example/escape", loaderId: "loader-b",
        frameId: "frame-a", source: "Page.frameNavigated",
      });
      if (bridge.state().lease) throw new Error("allowed-to-disallowed capture race retained authority");

      bridge.setLease();
      bridge.signal({
        tabId: 9, url: "https://allowed.example/replaced", loaderId: "loader-c",
        frameId: "frame-a", source: "Page.frameNavigated",
      });
      if (bridge.state().lease) throw new Error("prepare-to-input document replacement retained authority");

      bridge.setLease({
        kind: "navigate", expectedUrl: "https://allowed.example/b",
        authorizedAt: Date.now(), lastSignalAt: 0,
      });
      bridge.signal({
        tabId: 9, url: "https://allowed.example/b", loaderId: "loader-d",
        frameId: "frame-a", source: "Page.frameNavigated",
      });
      const intended = bridge.state().lease;
      if (!intended || intended.pendingNavigation || intended.loaderId !== "loader-d") {
        throw new Error("explicit intended navigation was not committed to the new loader");
      }
      // A committed top-level navigation destroys every isolated world, frame
      // agent, and child session, so frame support must be re-armed before the
      // next observation instead of reusing dead handles.
      const frames = bridge.frameState();
      if (frames.attached !== 0 || frames.snapshot !== null
        || frames.rootWorldContextId !== null || frames.supported !== false) {
        throw new Error(`frame sessions survived a top-level navigation: ${JSON.stringify(frames)}`);
      }
      if (!bridge.frameInvalidations().includes("top_level_navigation")) {
        throw new Error("a committed navigation did not invalidate the frame snapshot");
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run navigation epoch harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node navigation epoch harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn held_mouse_and_key_down_intents_survive_worker_restart() {
    let background = extension_source("background.js");
    assert!(background.contains("const CONTROL_INPUTS_KEY = \"browserControlHeldInputs\""));
    assert!(background.contains("[CONTROL_INPUTS_KEY]: {"));
    assert!(background.contains("mouse: [...heldMouseInputs]"));
    assert!(background.contains("keyboard: [...heldKeyInputs]"));
    assert!(background.contains("reason: \"recovered_held_input\""));
    assert!(background.contains("phase: \"held_input_cleanup\""));
    assert!(background.contains("heldMouseInputs.size > 0"));
    assert!(background.contains("heldKeyInputs.size > 0"));
    let recovery_start = background.find("if (invalidCandidate)").unwrap();
    let recovery_end = background[recovery_start..]
        .find("const targetsOperation")
        .unwrap()
        + recovery_start;
    let recovery = &background[recovery_start..recovery_end];
    assert!(
        recovery.find("heldMouseInputs.size > 0").unwrap()
            < recovery
                .find("await retryPendingControlCleanups()")
                .unwrap()
    );
    assert!(
        recovery
            .find("await retryPendingControlCleanups()")
            .unwrap()
            < recovery.find("boundedDebuggerDetach").unwrap()
    );

    for (function, map, down, up) in [
        (
            "async function trustedClick(",
            "heldMouseInputs",
            "mousePressed",
            "mouseReleased",
        ),
        (
            "async function trustedKey",
            "heldKeyInputs",
            "rawKeyDown",
            "keyUp",
        ),
    ] {
        let start = background.find(function).unwrap();
        let end = background[start..].find("\n}\n").unwrap() + start + 3;
        let body = &background[start..end];
        let persist = body.find(&format!("persistHeldInputIntent({map}")).unwrap();
        let down_dispatch = body.find(down).unwrap();
        let clear = body.find(&format!("clearHeldInputIntent({map}")).unwrap();
        let up_dispatch = body.rfind(up).unwrap();
        assert!(
            persist < down_dispatch,
            "{function} did not persist before down"
        );
        assert!(
            up_dispatch < clear,
            "{function} cleared intent before acknowledged up"
        );
    }

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const brace = source.indexOf("{", start);
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/background.js", "utf8");
      const functions = ["persistControlState", "persistHeldInputIntent"]
        .map((name) => extractFunction(source, name)).join("\n");
      const bridge = new Function(`
        const CONTROL_STORAGE_KEY = "lease";
        const CONTROL_REVOCATION_KEY = "revocation";
        const CONTROL_CLEANUPS_KEY = "cleanups";
        const CONTROL_INPUTS_KEY = "inputs";
        const CREATED_TABS_KEY = "tabs";
        let controlLease = { tabId: 14, sessionId: "session-14", epoch: 3 };
        let lastControlRevocation = null;
        const pendingControlCleanups = new Map();
        const bridgeCreatedTabs = new Set();
        const heldMouseInputs = new Map();
        const heldKeyInputs = new Map();
        let controlStateWrite = Promise.resolve();
        let durable = null;
        const chrome = { storage: { session: { set: async (value) => { durable = structuredClone(value); } } } };
        ${functions}
        return {
          persistMouse: (key, record) => persistHeldInputIntent(heldMouseInputs, key, record),
          persistKey: (key, record) => persistHeldInputIntent(heldKeyInputs, key, record),
          durable: () => durable,
        };
      `)();
      await bridge.persistMouse("mouse-intent", {
        tabId: 14, sessionId: "session-14", epoch: 3,
        releaseMethod: "Input.dispatchMouseEvent",
        releaseParams: { type: "mouseReleased", x: 12, y: 34, button: "left", clickCount: 1 },
      });
      await bridge.persistKey("key-intent", {
        tabId: 14, sessionId: "session-14", epoch: 3,
        releaseMethod: "Input.dispatchKeyEvent",
        releaseParams: { type: "keyUp", key: "Enter", code: "Enter" },
      });
      const stored = bridge.durable().inputs;
      if (stored.mouse.length !== 1 || stored.keyboard.length !== 1) {
        throw new Error("crash boundary lost a held input intent");
      }
      if (stored.mouse[0].releaseParams.type !== "mouseReleased"
        || stored.keyboard[0].releaseParams.type !== "keyUp") {
        throw new Error("restart cleanup did not retain exact release parameters");
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run held-input durability harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node held-input durability harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn websocket_auth_uses_token_free_mutual_hmac_challenge() {
    let background = extension_source("background.js");
    assert!(background.contains("new WebSocket(`ws://127.0.0.1:${config.port}/bridge`)"));
    assert!(!background.contains("/bridge?token="));
    assert!(background.contains("type: \"authHello\""));
    assert!(background.contains("message.type !== \"authChallenge\""));
    assert!(background.contains("type: \"authResponse\""));
    assert!(background.contains("crypto.subtle.verify(\"HMAC\""));
    assert!(background.contains("crypto.subtle.sign(\"HMAC\""));
    assert!(background.contains("LBB-WS-AUTH-V1\\nserver\\nbrowser-extension"));
    assert!(background.contains("LBB-WS-AUTH-V1\\nclient\\nbrowser-extension"));
    assert!(background.contains("AUTH_NEGOTIATION_TIMEOUT_MS = 3_000"));
    assert!(background.contains("AUTH_MAX_FRAME_BYTES = 8 * 1024"));
    assert!(background.contains("AUTH_MAX_INBOUND_FRAMES = 4"));
    assert!(background.contains("message.sessionId !== authSessionId"));

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const asyncMarker = `async function ${name}(`;
        const plainMarker = `function ${name}(`;
        const start = source.includes(asyncMarker)
          ? source.indexOf(asyncMarker)
          : source.indexOf(plainMarker);
        const brace = source.indexOf("{", start);
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/background.js", "utf8");
      const functions = [
        "decodeBase64Url32", "encodeBase64Url", "importAuthKey",
        "serverAuthPayload", "clientAuthPayload", "verifyAuthProof", "createAuthProof",
      ].map((name) => extractFunction(source, name)).join("\n");
      const auth = new Function(`${functions}; return {
        encodeBase64Url, importAuthKey, serverAuthPayload, clientAuthPayload,
        verifyAuthProof, createAuthProof,
      };`)();
      const token = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8";
      const key = await auth.importAuthKey(token);
      const session = "123e4567-e89b-12d3-a456-426614174000";
      const clientNonce = "ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8";
      const serverNonce = "QEFCQ0RFRkdISUpLTE1OT1BRUlNUVVZXWFlaW1xdXl8";
      const serverPayload = auth.serverAuthPayload(session, clientNonce, serverNonce);
      const serverProof = await auth.createAuthProof(key, serverPayload);
      if (serverProof !== "kCFSEQMkA3UFR8ONc1oTIET5-XtGDT6K-qHpqxcBoC0") {
        throw new Error("server proof diverged from the Rust cross-language vector");
      }
      if (!await auth.verifyAuthProof(key, serverProof, serverPayload)) {
        throw new Error("valid server HMAC was rejected");
      }
      if (await auth.verifyAuthProof(key, serverProof, `${serverPayload}x`)) {
        throw new Error("altered server challenge passed HMAC verification");
      }
      const replayNonce = auth.encodeBase64Url(new Uint8Array(32).fill(8));
      const replayPayload = auth.serverAuthPayload(session, replayNonce, serverNonce);
      if (await auth.verifyAuthProof(key, serverProof, replayPayload)) {
        throw new Error("harvested server challenge replayed against a fresh client nonce");
      }
      const correctedExpected = `LBB-WS-AUTH-V1\nclient\nbrowser-extension\n${session}\n${clientNonce}\n${serverNonce}`;
      if (auth.clientAuthPayload(session, clientNonce, serverNonce) !== correctedExpected) {
        throw new Error("client canonical payload changed");
      }
      const clientProof = await auth.createAuthProof(key, correctedExpected);
      if (clientProof !== "aLkKO_2gRdXF217mxjepybU7h-Cdu3rVqK-U_wpauvY") {
        throw new Error("client proof diverged from the Rust cross-language vector");
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run mutual-auth harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node mutual-auth harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn websocket_envelopes_are_versioned_ordered_and_session_bound() {
    let background = extension_source("background.js");
    let library = extension_source("lib.js");
    assert!(library.contains("export const PROTOCOL_VERSION = 1"));
    assert!(background.contains("protocolVersion: PROTOCOL_VERSION"));
    assert!(background.contains("message.type !== \"authChallenge\""));
    assert!(background.contains("message.type !== \"welcome\""));
    assert!(background.contains("message.type !== \"helloAck\""));
    assert!(background.contains("sessionId: protocolServerSessionId"));
    assert!(background.contains("controllerId"));
    assert!(background.contains("connectionId: protocolConnectionId"));
    assert!(background.contains("controllerSequence: outboundSequence"));
    assert!(background.contains("eventSequence"));
    assert!(background.contains("sequence: message.sequence"));
    assert!(background.contains("message.sessionId !== protocolServerSessionId"));
    assert!(background.contains("message.sequence <= lastCommandSequence"));
}

#[test]
fn wait_for_is_read_only_pause_allowed_and_time_bounded() {
    let background = extension_source("background.js");
    let content = extension_source("content.js");

    // page.waitFor is read-only, so it stays usable during a human pause.
    let pause_allowed = background
        .split("const PAUSE_ALLOWED_COMMANDS = new Set([")
        .nth(1)
        .unwrap()
        .split("]);")
        .next()
        .unwrap();
    assert!(pause_allowed.contains("\"page.waitFor\""));

    let case_start = background.find("case \"page.waitFor\"").unwrap();
    let case_end = background[case_start + 5..]
        .find("\n    case \"")
        .map(|offset| case_start + 5 + offset)
        .unwrap_or(background.len());
    let case_body = &background[case_start..case_end];
    assert!(case_body.contains("getTab(params.tabId)"));
    assert!(case_body.contains("assertAllowedTab(tab)"));
    assert!(case_body.contains("WAIT_CONTENT_TIMEOUT_MARGIN_MS"));
    assert!(case_body.contains("retryAfterTimeout: false"));
    // No control lease, no generation, and never any trusted input.
    assert!(!case_body.contains("requireControl"));
    assert!(!case_body.contains("captureLeaseAuthority"));
    assert!(!case_body.contains("generation"));
    assert!(!case_body.contains("Input.dispatch"));

    // The dedicated content bound must exceed the maximum 12s wait while
    // staying under the server's 15s command deadline.
    assert!(background.contains("const WAIT_CONTENT_TIMEOUT_MARGIN_MS = 1_500"));
    assert!(background.contains("const WAIT_TIMEOUT_MAX_MS = 12_000"));
    assert!(background.contains("timeoutMs: timeoutMs + WAIT_CONTENT_TIMEOUT_MARGIN_MS"));
    assert!(background.contains("timeoutMs = CONTENT_TIMEOUT_MS"));
    // A canceled wait never revokes an unrelated control lease.
    assert!(background.contains("method.startsWith(\"page.\") && method !== \"page.waitFor\""));

    assert!(content.contains("case \"wait\""));
    let wait_start = content.find("function pageContainsText").unwrap();
    let wait_end = content.find("function setNativeValue").unwrap();
    let wait_body = &content[wait_start..wait_end];
    assert!(wait_body.contains("WAIT_POLL_INTERVAL_MS"));
    assert!(wait_body.contains("revisions.read()"));
    assert!(wait_body.contains("WAIT_TIMEOUT:"));
    assert!(!wait_body.contains("dispatchEvent"));
    assert!(!wait_body.contains("Input.dispatch"));

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const signatureEnd = source.indexOf(") {", start);
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/content.js", "utf8");
      const functions = ["evaluateWaitConditions", "waitForConditions"]
        .map((name) => extractFunction(source, name)).join("\n");
      const bridge = new Function(`
        const WAIT_POLL_INTERVAL_MS = 250;
        const WAIT_TIMEOUT_DEFAULT_MS = 5_000;
        const WAIT_TIMEOUT_MIN_MS = 100;
        const WAIT_TIMEOUT_MAX_MS = 12_000;
        let clock = 0;
        let documentRevision = 0;
        const revisions = { read: () => documentRevision };
        let pageText = "Loading spinner";
        let inputDispatches = 0;
        const scheduled = [];
        const location = { href: "https://example.test/start" };
        const Date = { now: () => clock };
        function pageContainsText(needle) { return pageText.includes(needle); }
        function debuggerCommand() { inputDispatches += 1; return Promise.resolve(); }
        function waitDelay(milliseconds) {
          clock += milliseconds;
          for (const step of scheduled) {
            if (!step.done && clock >= step.at) { step.done = true; step.run(); }
          }
          return Promise.resolve();
        }
        ${functions}
        return {
          wait: (message) => waitForConditions(message),
          scheduleText(at, text) { scheduled.push({ at, done: false, run: () => { pageText = text; } }); },
          scheduleRevision(at) { scheduled.push({ at, done: false, run: () => { documentRevision += 1; } }); },
          state: () => ({ clock, inputDispatches }),
        };
      `)();

      bridge.scheduleText(600, "Welcome back");
      const appeared = await bridge.wait({ text: "Welcome", timeoutMs: 5_000 });
      if (appeared.satisfied !== true || appeared.conditions.text !== true
        || !(appeared.elapsedMs > 0) || appeared.elapsedMs > 1_000) {
        throw new Error(`appearing text did not satisfy the wait: ${JSON.stringify(appeared)}`);
      }

      const timeoutStart = bridge.state().clock;
      let timedOut = null;
      try {
        await bridge.wait({ text: "Never rendered", timeoutMs: 1_000 });
      } catch (error) {
        timedOut = error.message;
      }
      if (!timedOut || !timedOut.startsWith("WAIT_TIMEOUT:")
        || !timedOut.includes("satisfied: false") || !timedOut.includes("elapsedMs")) {
        throw new Error(`missing coaching WAIT_TIMEOUT: ${timedOut}`);
      }
      if (bridge.state().clock - timeoutStart < 1_000) {
        throw new Error("the wait gave up before its timeout elapsed");
      }

      bridge.scheduleRevision(bridge.state().clock + 200);
      bridge.scheduleRevision(bridge.state().clock + 400);
      const quiet = await bridge.wait({ mutationQuietMs: 300, timeoutMs: 2_000 });
      if (quiet.satisfied !== true || quiet.conditions.mutationQuiet !== true) {
        throw new Error(`mutation quiet was not detected: ${JSON.stringify(quiet)}`);
      }

      const prefixed = await bridge.wait({ urlPrefix: "https://example.test/", timeoutMs: 500 });
      if (prefixed.satisfied !== true || prefixed.conditions.urlPrefix !== true) {
        throw new Error("urlPrefix condition failed on a matching href");
      }

      let unconditional = false;
      try { await bridge.wait({ timeoutMs: 500 }); }
      catch (error) { unconditional = error.message.startsWith("BAD_REQUEST:"); }
      if (!unconditional) throw new Error("a conditionless wait was accepted");

      if (bridge.state().inputDispatches !== 0) {
        throw new Error("the wait loop dispatched trusted input");
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run wait-loop harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node wait-loop harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn epoch_embedded_refs_fail_stale_with_coaching_before_any_lookup() {
    let content = extension_source("content.js");
    assert!(content.contains("const ref = `${generation}.${key}`"));
    assert!(
        content.contains("superseded by ${generation}; observe the page again and use fresh refs")
    );
    let resolve_start = content.find("function resolveRecord").unwrap();
    let resolve_end = content[resolve_start..].find("\n  }\n").unwrap() + resolve_start;
    let resolve = &content[resolve_start..resolve_end];
    assert!(
        resolve.find("assertEmbeddedGeneration(ref)").unwrap()
            < resolve.find("refs.get(key)").unwrap()
    );

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const signatureEnd = source.indexOf(") {", start);
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/content.js", "utf8");
      const functions = ["parseElementRef", "assertEmbeddedGeneration", "resolveRecord"]
        .map((name) => extractFunction(source, name)).join("\n");
      const bridge = new Function(`
        let generation = "gen-b1";
        const lookups = [];
        const element = { isConnected: true };
        const records = new Map([["e1", { element, ref: "gen-b1.e1" }]]);
        const refs = { get(key) { lookups.push(key); return records.get(key); } };
        function assertFresh() {}
        ${functions}
        return {
          resolve: (ref) => resolveRecord(ref, "gen-b1"),
          lookups: () => [...lookups],
        };
      `)();

      let staleMessage = null;
      try { bridge.resolve("gen-a0.e1"); }
      catch (error) { staleMessage = error.message; }
      if (staleMessage !== "STALE_REF: snapshot gen-a0 superseded by gen-b1; observe the page again and use fresh refs") {
        throw new Error(`stale embedded ref lacked coaching: ${staleMessage}`);
      }
      if (bridge.lookups().length !== 0) {
        throw new Error("a stale embedded ref touched the refs map");
      }

      const fresh = bridge.resolve("gen-b1.e1");
      if (fresh.ref !== "gen-b1.e1" || bridge.lookups().join() !== "e1") {
        throw new Error("a fresh embedded ref did not resolve against the current snapshot");
      }

      const legacy = bridge.resolve("e1");
      if (legacy.ref !== "gen-b1.e1" || bridge.lookups().join() !== "e1,e1") {
        throw new Error("a legacy bare ref did not resolve against the current generation");
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run embedded-ref harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node embedded-ref harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn hover_moves_the_trusted_pointer_without_press_events() {
    let background = extension_source("background.js");
    let case_start = background.find("case \"page.hover\"").unwrap();
    let case_end = background[case_start + 5..]
        .find("\n    case \"")
        .map(|offset| case_start + 5 + offset)
        .unwrap_or(background.len());
    let case_body = &background[case_start..case_end];
    assert!(case_body.contains("requireControl"));
    assert!(case_body.contains("assertControlBinding(params)"));
    assert!(case_body.contains("method: \"prepareClick\""));
    assert!(case_body.contains("ELEMENT_DISABLED"));
    assert!(case_body.contains("trustedHover"));

    let hover_start = background.find("async function trustedHover(").unwrap();
    let hover_end = background[hover_start..].find("\n}\n").unwrap() + hover_start + 3;
    let hover = &background[hover_start..hover_end];
    assert!(hover.contains("moveVirtualCursor"));
    assert!(hover.contains("assertPointerArrival(motion)"));
    assert!(!hover.contains("mousePressed"));
    assert!(!hover.contains("mouseReleased"));
    assert!(!hover.contains("Input.dispatchMouseEvent"));
    assert!(!hover.contains("persistHeldInputIntent"));

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const signatureEnd = source.indexOf(") {", start);
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/background.js", "utf8");
      const functions = ["assertPointerArrival", "trustedHover"]
        .map((name) => extractFunction(source, name)).join("\n");
      const bridge = new Function(`
        const dispatched = [];
        let arrival = "arrived";
        function captureLeaseAuthority() { return { tabId: 9, sessionId: "lease-9", epoch: 3 }; }
        function verifyDocumentAuthority() { return Promise.resolve(); }
        function verifyDocumentAuthorityAfterDispatch() { return Promise.resolve(); }
        function moveVirtualCursor(_tabId, x, y) {
          dispatched.push({ kind: "move", x, y });
          return Promise.resolve({ arrival, moveSequence: 8 });
        }
        function debuggerCommand(_tabId, method, params) {
          dispatched.push({ kind: "cdp", method, type: params?.type });
          return Promise.resolve();
        }
        function publicControlState() { return { active: true }; }
        ${functions}
        return {
          hover: () => trustedHover(9, { bounds: { x: 10, y: 20, width: 30, height: 40 } }, "gen.e1", "gen"),
          loseArrival() { arrival = "lost"; },
          dispatched: () => [...dispatched],
        };
      `)();

      const result = await bridge.hover();
      if (result.hovered !== true || result.x !== 25 || result.y !== 40 || result.motion.moveSequence !== 8) {
        throw new Error(`hover did not settle on the target center: ${JSON.stringify(result)}`);
      }
      const moves = bridge.dispatched();
      if (moves.length !== 1 || moves[0].kind !== "move" || moves[0].x !== 25 || moves[0].y !== 40) {
        throw new Error("hover dispatched something besides one pointer movement");
      }
      if (moves.some((entry) => entry.type === "mousePressed" || entry.type === "mouseReleased")) {
        throw new Error("hover dispatched a press event");
      }

      bridge.loseArrival();
      let refused = false;
      try { await bridge.hover(); }
      catch (error) { refused = error.message.startsWith("POINTER_NOT_ARRIVED:"); }
      if (!refused) throw new Error("hover accepted an unacknowledged pointer arrival");
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run hover harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node hover harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn click_pointer_options_stay_gated_and_proof_ordered() {
    let background = extension_source("background.js");
    assert!(
        background
            .contains("const POINTER_MODIFIER_BITS = { Alt: 1, Control: 2, Meta: 4, Shift: 8 }")
    );

    // The two-phase proof is unchanged: the commit proof always precedes the
    // trusted mouse press, whatever button, count, or modifiers are used.
    let click_start = background.find("async function trustedClick(").unwrap();
    let click_end = background[click_start..].find("\n}\n").unwrap() + click_start + 3;
    let click = &background[click_start..click_end];
    assert!(click.find("method: \"commitClick\"").unwrap() < click.find("mousePressed").unwrap());
    assert!(click.contains("{ type: \"mousePressed\", x, y, button, clickCount, modifiers }"));
    assert!(click.contains("{ type: \"mouseReleased\", x, y, button, clickCount, modifiers }"));
    assert!(click.contains("BAD_BUTTON"));
    assert!(click.contains("BAD_CLICK_COUNT"));

    // Safe mode routes every non-default pointer verb through the existing
    // risky-click approval path; Full Access stays unrestricted.
    let case_start = background.find("case \"page.click\"").unwrap();
    let case_end = background[case_start + 5..]
        .find("\n    case \"")
        .map(|offset| case_start + 5 + offset)
        .unwrap_or(background.len());
    let case_body = &background[case_start..case_end];
    assert!(case_body.contains("pointerModifierMask"));
    assert!(case_body.contains("perform a modified pointer click"));
    assert!(case_body.contains("classifyRisk(description) ?? pointerRisk"));
    assert!(case_body.contains("!config.fullAccess && risk && !approved"));
    assert!(case_body.find("pointerRisk").unwrap() < case_body.find("queueApproval").unwrap());

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const signatureEnd = source.indexOf(") {", start);
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/background.js", "utf8");
      const pointerModifierMask = extractFunction(source, "pointerModifierMask");
      const bridge = new Function(`
        const POINTER_MODIFIER_BITS = { Alt: 1, Control: 2, Meta: 4, Shift: 8 };
        ${pointerModifierMask}
        return { mask: pointerModifierMask };
      `)();
      if (bridge.mask([]) !== 0) throw new Error("no modifiers must map to an empty bitmask");
      if (bridge.mask(["Shift"]) !== 8 || bridge.mask(["Alt"]) !== 1
        || bridge.mask(["Control"]) !== 2 || bridge.mask(["Meta"]) !== 4) {
        throw new Error("single modifiers diverged from the CDP bitmask");
      }
      if (bridge.mask(["Shift", "Meta"]) !== 12 || bridge.mask(["Control", "Alt", "Shift"]) !== 11) {
        throw new Error("combined modifiers diverged from the CDP bitmask");
      }
      let refused = false;
      try { bridge.mask(["Hyper"]); }
      catch (error) { refused = error.message.startsWith("BAD_MODIFIER:"); }
      if (!refused) throw new Error("an unknown modifier was accepted");
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run modifier bitmask harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node modifier bitmask harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn batch_delegates_sequentially_and_stops_at_the_first_failure() {
    let background = extension_source("background.js");

    // page.batch and page.handleDialog are mutating commands, so neither may
    // run while a human has paused remote control.
    let pause_allowed = background
        .split("const PAUSE_ALLOWED_COMMANDS = new Set([")
        .nth(1)
        .unwrap()
        .split("]);")
        .next()
        .unwrap();
    assert!(!pause_allowed.contains("\"page.batch\""));
    assert!(!pause_allowed.contains("\"page.handleDialog\""));

    assert!(background.contains("const BATCH_MAX_ACTIONS = 10"));
    assert!(background.contains(
        "const BATCH_SUBMETHODS = new Set([\"page.click\", \"page.fill\", \"page.select\", \"page.key\", \"page.scroll\"])"
    ));

    let case_start = background.find("case \"page.batch\"").unwrap();
    let case_end = background[case_start + 5..]
        .find("\n    case \"")
        .map(|offset| case_start + 5 + offset)
        .unwrap_or(background.len());
    let case_body = &background[case_start..case_end];
    assert!(case_body.contains("requireControl"));
    assert!(case_body.contains("assertControlBinding(params)"));
    assert!(case_body.contains("BATCH_MAX_ACTIONS"));
    assert!(case_body.contains("runBatchActions"));
    // Sub-actions re-enter dispatch with the same command context and the
    // batched marker, so every existing per-method proof runs unchanged.
    assert!(
        case_body
            .contains("dispatch(subMethod, subParams, false, commandContext, { batched: true })")
    );

    // A Safe-mode approval fails the batch at its index instead of queueing.
    let click_start = background.find("case \"page.click\"").unwrap();
    let click_end = background[click_start + 5..]
        .find("\n    case \"")
        .map(|offset| click_start + 5 + offset)
        .unwrap_or(background.len());
    let click_body = &background[click_start..click_end];
    assert!(click_body.contains("if (batched)"));
    assert!(click_body.contains("APPROVAL_REQUIRED"));
    assert!(
        click_body.find("APPROVAL_REQUIRED").unwrap() < click_body.find("queueApproval").unwrap()
    );

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const signatureEnd = source.indexOf(") {", start);
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/background.js", "utf8");
      const runBatchActions = extractFunction(source, "runBatchActions");
      const bridge = new Function(`
        const BATCH_SUBMETHODS = new Set(["page.click", "page.fill", "page.select", "page.key", "page.scroll"]);
        let controlLease = null;
        ${runBatchActions}
        return {
          run: runBatchActions,
          setLease(lease) { controlLease = lease; },
        };
      `)();

      // Every step succeeds: no failedIndex, ordered per-step results.
      let calls = [];
      const all = await bridge.run(
        [
          { method: "page.fill", ref: "e1", text: "a", tabId: 7, generation: "g1" },
          { method: "page.key", key: "Enter", tabId: 7, generation: "g1" },
        ],
        async (method, params) => { calls.push({ method, params: { ...params } }); },
      );
      if (all.completed !== 2 || all.total !== 2 || "failedIndex" in all
        || all.perStep.length !== 2 || !all.perStep.every((step) => step.ok === true)) {
        throw new Error(`a clean batch misreported: ${JSON.stringify(all)}`);
      }
      if (calls.length !== 2 || calls[0].method !== "page.fill"
        || calls[0].params.method !== undefined || calls[0].params.ref !== "e1") {
        throw new Error("sub-params were not delegated without the method field");
      }

      // The loop stops at the first failing step and never dispatches later
      // steps: stale-snapshot strictness is preserved, not weakened.
      calls = [];
      const failed = await bridge.run(
        [
          { method: "page.fill", ref: "e1", text: "a" },
          { method: "page.scroll", deltaY: 120 },
          { method: "page.click", ref: "e2" },
          { method: "page.click", ref: "e3" },
        ],
        async (method) => {
          calls.push(method);
          if (calls.length === 3) throw new Error("STALE_SNAPSHOT: observe the page again before acting");
        },
      );
      if (failed.completed !== 2 || failed.total !== 4 || failed.failedIndex !== 2
        || !failed.failedError.startsWith("STALE_SNAPSHOT:")
        || failed.perStep.length !== 3 || failed.perStep[2].ok !== false
        || failed.perStep[2].error !== failed.failedError) {
        throw new Error(`stop-at-first-failure misreported: ${JSON.stringify(failed)}`);
      }
      if (calls.length !== 3) {
        throw new Error("a sub-action was dispatched after the batch aborted");
      }

      // Forbidden sub-methods fail the batch at their index without any
      // dispatch of the forbidden or later steps.
      calls = [];
      const forbidden = await bridge.run(
        [
          { method: "page.fill", ref: "e1", text: "a" },
          { method: "page.evaluate", expression: "1" },
          { method: "page.click", ref: "e2" },
        ],
        async (method) => { calls.push(method); },
      );
      if (forbidden.completed !== 1 || forbidden.failedIndex !== 1
        || !forbidden.failedError.startsWith("BAD_REQUEST:")
        || forbidden.perStep.length !== 2 || calls.join() !== "page.fill") {
        throw new Error(`a forbidden sub-method leaked: ${JSON.stringify(forbidden)}`);
      }
      const nested = await bridge.run(
        [{ method: "page.batch", actions: [] }],
        async () => { throw new Error("nested batch must never dispatch"); },
      );
      if (nested.completed !== 0 || nested.failedIndex !== 0
        || !nested.failedError.startsWith("BAD_REQUEST:")) {
        throw new Error(`a nested batch was not refused: ${JSON.stringify(nested)}`);
      }

      // A dialog opened by a sub-step freezes the renderer, so the loop
      // checks the synchronous pendingDialog record before EACH later step
      // and aborts at that exact index without dispatching into the frozen
      // page.
      calls = [];
      const lease = { pendingDialog: null };
      bridge.setLease(lease);
      const dialoged = await bridge.run(
        [
          { method: "page.fill", ref: "e1", text: "a" },
          { method: "page.click", ref: "e2" },
          { method: "page.key", key: "Enter" },
        ],
        async (method) => {
          calls.push(method);
          if (calls.length === 1) lease.pendingDialog = { type: "confirm" };
        },
      );
      if (dialoged.completed !== 1 || dialoged.failedIndex !== 1
        || !dialoged.failedError.startsWith("BLOCKED_BY_DIALOG:")
        || dialoged.perStep.length !== 2 || dialoged.perStep[1].ok !== false
        || calls.join() !== "page.fill") {
        throw new Error(`a mid-batch dialog did not abort the batch: ${JSON.stringify(dialoged)}`);
      }
      bridge.setLease(null);
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run batch loop harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node batch loop harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn dialog_interception_records_events_and_handles_dialogs_lease_bound() {
    let background = extension_source("background.js");

    // The Page domain is enabled once per lease, so dialog lifecycle events
    // are already flowing whenever a lease is active.
    assert!(
        background.contains(
            "await debuggerCommand(tab.id, \"Page.enable\", {}, authority, commandContext)"
        )
    );

    // Dialog listeners live on the lease-scoped debugger event stream.
    let listener_start = background
        .find("chrome.debugger.onEvent.addListener")
        .unwrap();
    let listener_end = background[listener_start..]
        .find("chrome.tabs.onUpdated.addListener")
        .unwrap()
        + listener_start;
    let listener = &background[listener_start..listener_end];
    assert!(listener.contains("controlLease?.tabId !== source.tabId"));
    assert!(listener.contains("Page.javascriptDialogOpening"));
    assert!(listener.contains("Page.javascriptDialogClosed"));
    assert!(listener.contains("name: \"page.dialogOpened\""));
    assert!(listener.contains("name: \"page.dialogClosed\""));
    assert!(listener.contains("DIALOG_MESSAGE_MAX_CHARS"));
    assert!(listener.contains("controlLease.pendingDialog = null"));
    assert!(background.contains("const DIALOG_MESSAGE_MAX_CHARS = 500"));
    assert!(background.contains("const DIALOG_PROMPT_TEXT_MAX_CHARS = 1_000"));

    // page.handleDialog is a lease-bound CDP action; it deliberately skips
    // the document identity precheck so beforeunload dialogs (which hold the
    // document in a pending-navigation state) stay handleable, and the
    // documented safe default for them is accept:false.
    let case_start = background.find("case \"page.handleDialog\"").unwrap();
    let case_end = background[case_start + 5..]
        .find("\n    case \"")
        .map(|offset| case_start + 5 + offset)
        .unwrap_or(background.len());
    let case_body = &background[case_start..case_end];
    assert!(case_body.contains("requireControl"));
    assert!(case_body.contains("assertControlBinding(params)"));
    assert!(case_body.contains("captureLeaseAuthority"));
    assert!(case_body.contains("NO_PENDING_DIALOG"));
    assert!(case_body.contains("Page.handleJavaScriptDialog"));
    assert!(case_body.contains("params.accept === true"));
    assert!(case_body.contains("DIALOG_PROMPT_TEXT_MAX_CHARS"));
    assert!(case_body.contains("await clearPendingDialog(authority)"));
    assert!(!case_body.contains("verifyDocumentAuthority"));

    // Clearing the record is lease-bound and unconditional: nothing about the
    // resolved dialog is remembered, so the next document-identity boundary
    // runs unforgiven.
    let clear_start = background
        .find("async function clearPendingDialog")
        .unwrap();
    let clear_end = background[clear_start..].find("\n}\n").unwrap() + clear_start;
    let clear = &background[clear_start..clear_end];
    assert!(clear.contains("controlLease?.sessionId !== authority.sessionId"));
    assert!(clear.contains("controlLease.pendingDialog = null"));
    assert!(!clear.contains("assertDialogNotBlocking"));

    // Lease revocation drops the lease object, and the pending dialog record
    // lives on the lease, so revocation clears it on the extension side too.
    assert!(background.contains("pendingDialog: null"));
}

#[test]
fn pending_dialogs_fail_fast_and_never_revoke_the_lease_by_timeout() {
    let background = extension_source("background.js");

    // The extension mirrors the server's dialog gate exactly: only these
    // five renderer-free commands stay dispatchable while a dialog blocks
    // the controlled page.
    let tolerant = background
        .split("const DIALOG_TOLERANT_COMMANDS = new Set([")
        .nth(1)
        .unwrap()
        .split("]);")
        .next()
        .unwrap();
    for allowed in [
        "status",
        "tabs.list",
        "browser.control.status",
        "browser.control.stop",
        "page.handleDialog",
    ] {
        assert!(tolerant.contains(&format!("\"{allowed}\"")));
    }
    for blocked in [
        "page.observe",
        "page.waitFor",
        "browser.control.start",
        "page.click",
        "page.batch",
        "tabs.activate",
    ] {
        assert!(!tolerant.contains(&format!("\"{blocked}\"")));
    }

    // dispatch fails renderer-touching methods fast, before any content or
    // CDP call in the per-method case bodies.
    let dispatch_start = background.find("async function dispatch").unwrap();
    let dispatch_end = background[dispatch_start..]
        .find("async function popupState")
        .unwrap()
        + dispatch_start;
    let dispatch = &background[dispatch_start..dispatch_end];
    assert!(
        dispatch.find("assertNoPendingDialog(method)").unwrap()
            < dispatch.find("switch (method)").unwrap()
    );

    // The batch loop consults the synchronous pendingDialog record before
    // every sub-step, ahead of the sub-method allowlist check.
    let batch_start = background.find("async function runBatchActions").unwrap();
    let batch_end = background[batch_start..].find("\n}\n").unwrap() + batch_start;
    let batch = &background[batch_start..batch_end];
    assert!(
        batch.find("controlLease?.pendingDialog").unwrap()
            < batch.find("BATCH_SUBMETHODS.has(method)").unwrap()
    );

    // The heartbeat keeps its browser-side attachment and document checks
    // but skips the renderer-touching Runtime.evaluate probe and overlay
    // refresh while the dialog is pending, and a dialog-stalled authorized
    // navigation is not a navigation timeout.
    let heartbeat_start = background.find("async function heartbeatControl").unwrap();
    let heartbeat_end = background[heartbeat_start..].find("\n}\n").unwrap() + heartbeat_start;
    let heartbeat = &background[heartbeat_start..heartbeat_end];
    assert!(heartbeat.contains("verifyDocumentAuthority"));
    assert!(
        heartbeat.find("if (lease.pendingDialog)").unwrap()
            < heartbeat.find("Runtime.evaluate").unwrap()
    );
    assert!(heartbeat.contains("!lease.pendingDialog && Date.now()"));
    assert!(heartbeat.contains("BLOCKED_BY_DIALOG"));

    // Timeouts that fire while a dialog is pending resolve as
    // BLOCKED_BY_DIALOG without the lease-revoking stopControl paths.
    let bounded_start = background
        .find("async function boundedContentOperation")
        .unwrap();
    let bounded_end = background[bounded_start..].find("\n}\n").unwrap() + bounded_start;
    let bounded = &background[bounded_start..bounded_end];
    assert!(
        bounded
            .find("error.code === \"CONTENT_TIMEOUT\" && controlLease?.pendingDialog")
            .unwrap()
            < bounded.find("stopControl").unwrap()
    );
    let cdp_start = background.find("async function debuggerCommand").unwrap();
    let cdp_end = background[cdp_start..].find("\n}\n").unwrap() + cdp_start;
    let cdp = &background[cdp_start..cdp_end];
    assert!(cdp.find("controlLease?.pendingDialog").unwrap() < cdp.find("stopControl").unwrap());

    // The 0.10 gate hardened only the TIMEOUT routes, and the live 0.10.0
    // browser run then lost a lease through the document-identity route
    // instead. So the shared revocation boundary carries the same guard: a
    // document-authority failure taken under a pending dialog can never
    // revoke, whichever boundary funnelled into it.
    let fail_start = background
        .find("async function failChangedDocument")
        .unwrap();
    let fail_end = background[fail_start..].find("\n}\n").unwrap() + fail_start;
    let fail = &background[fail_start..fail_end];
    assert!(
        fail.find("assertDialogNotBlocking(boundary)").unwrap() < fail.find("stopControl").unwrap()
    );

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (start < 0) throw new Error(`missing ${name}`);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const signatureEnd = source.indexOf(") {", start);
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/background.js", "utf8");
      const setStart = source.indexOf("const DIALOG_TOLERANT_COMMANDS = new Set([");
      const setEnd = source.indexOf("]);", setStart) + 3;
      const tolerantSet = source.slice(setStart, setEnd);
      const pendingDialogError = extractFunction(source, "pendingDialogError");
      const assertNoPendingDialog = extractFunction(source, "assertNoPendingDialog");

      // Dispatch-level fast fail: renderer-touching methods throw
      // BLOCKED_BY_DIALOG while a dialog is pending; the tolerant five and a
      // dialog-free lease pass through untouched.
      const gate = new Function(`
        ${tolerantSet}
        let controlLease = null;
        ${pendingDialogError}
        ${assertNoPendingDialog}
        return {
          setLease(lease) { controlLease = lease; },
          check(method) { assertNoPendingDialog(method); },
        };
      `)();
      gate.setLease({ pendingDialog: { type: "confirm" } });
      for (const blocked of ["page.observe", "page.waitFor", "page.click", "page.batch", "tabs.activate", "browser.control.start"]) {
        let failed = null;
        try { gate.check(blocked); } catch (error) { failed = error; }
        if (failed?.code !== "BLOCKED_BY_DIALOG") {
          throw new Error(`${blocked} was not fast-failed under a pending dialog`);
        }
      }
      for (const allowed of ["status", "tabs.list", "browser.control.status", "browser.control.stop", "page.handleDialog"]) {
        gate.check(allowed);
      }
      gate.setLease(null);
      gate.check("page.click");

      // A content timeout under a pending dialog resolves BLOCKED_BY_DIALOG
      // without the lease-revoking stopControl; without a dialog the
      // outcome-unknown revocation path is unchanged.
      const boundedContentOperation = extractFunction(source, "boundedContentOperation");
      const content = new Function(`
        const CONTENT_TIMEOUT_MS = 6_000;
        let controlLease = null;
        let stopCalls = 0;
        function withCommandCancellation(promise) { return promise; }
        function withTimeout() {
          const error = new Error("CONTENT_TIMEOUT: content snapshot exceeded 6000ms");
          error.code = "CONTENT_TIMEOUT";
          return Promise.reject(error);
        }
        function stopControl() { stopCalls += 1; return Promise.resolve(); }
        function outcomeUnknownError(boundary, cause) {
          const error = new Error("ACTION_OUTCOME_UNKNOWN: " + boundary);
          error.code = "ACTION_OUTCOME_UNKNOWN";
          error.cause = cause;
          return error;
        }
        ${pendingDialogError}
        ${boundedContentOperation}
        return {
          setLease(lease) { controlLease = lease; },
          run: () => boundedContentOperation(Promise.resolve(), "content snapshot", { sessionId: "s" }, null),
          stops: () => stopCalls,
        };
      `)();
      content.setLease({ pendingDialog: { type: "alert" } });
      let dialogFailure = null;
      try { await content.run(); } catch (error) { dialogFailure = error; }
      if (dialogFailure?.code !== "BLOCKED_BY_DIALOG" || content.stops() !== 0) {
        throw new Error("a content timeout under a dialog revoked the lease or misclassified");
      }
      content.setLease(null);
      let unknownFailure = null;
      try { await content.run(); } catch (error) { unknownFailure = error; }
      if (unknownFailure?.code !== "ACTION_OUTCOME_UNKNOWN" || content.stops() !== 1) {
        throw new Error("a dialog-free content timeout lost its outcome-unknown revocation");
      }

      // The same holds for a CDP timeout racing a dialog.
      const debuggerCommand = extractFunction(source, "debuggerCommand");
      const commandTimeoutMs = extractFunction(source, "commandTimeoutMs");
      const frameObserveTimeoutError = extractFunction(source, "frameObserveTimeoutError");
      const cdp = new Function(`
        const DEBUGGER_TIMEOUT_MS = 10_000;
        const FRAME_OBSERVE_MIN_TIMEOUT_MS = 100;
        let controlLease = null;
        let stopCalls = 0;
        const timeouts = [];
        const chrome = { debugger: { sendCommand: () => new Promise(() => {}) } };
        function assertLeaseAuthority() {}
        function assertCommandActive() {}
        function assertLeaseAuthorityAfterDispatch() {}
        function withTimeout(_operation, timeoutMs, method) {
          timeouts.push(timeoutMs);
          const error = new Error("DEBUGGER_TIMEOUT: " + method + " exceeded 10ms");
          error.code = "DEBUGGER_TIMEOUT";
          return Promise.reject(error);
        }
        function stopControl() { stopCalls += 1; return Promise.resolve(); }
        ${pendingDialogError}
        ${commandTimeoutMs}
        ${frameObserveTimeoutError}
        ${debuggerCommand}
        return {
          setLease(lease) { controlLease = lease; },
          run: (options) => debuggerCommand(9, "Runtime.evaluate", {}, { sessionId: "s" }, null, null, options),
          stops: () => stopCalls,
          timeouts: () => [...timeouts],
        };
      `)();
      cdp.setLease({ pendingDialog: { type: "beforeunload" } });
      let cdpDialogFailure = null;
      try { await cdp.run(); } catch (error) { cdpDialogFailure = error; }
      if (cdpDialogFailure?.code !== "BLOCKED_BY_DIALOG" || cdp.stops() !== 0) {
        throw new Error("a CDP timeout under a dialog revoked the lease or misclassified");
      }
      cdp.setLease(null);
      let cdpUnknownFailure = null;
      try { await cdp.run(); } catch (error) { cdpUnknownFailure = error; }
      if (cdpUnknownFailure?.code !== "CDP_OUTCOME_UNKNOWN" || cdp.stops() !== 1) {
        throw new Error("a dialog-free CDP timeout lost its outcome-unknown revocation");
      }

      // A read-only frame-observation command carries the observation
      // deadline and is never lease-fatal: a slow third-party iframe costs
      // that frame, not the lease.
      let observeFailure = null;
      try { await cdp.run({ deadlineAt: Date.now() + 5_000, timeoutIsFatal: false }); }
      catch (error) { observeFailure = error; }
      if (observeFailure?.code !== "FRAME_OBSERVE_TIMEOUT" || cdp.stops() !== 1) {
        throw new Error(`a frame observation timeout revoked the lease: ${observeFailure && observeFailure.code}`);
      }
      // Every command is bounded by what is left of the shared deadline, so
      // sixteen slow frames cannot stretch one observation command by command.
      const spentDeadline = cdp.timeouts().at(-1);
      if (!(spentDeadline > 4_000) || spentDeadline > 5_000) {
        throw new Error(`a frame observation command ignored the shared deadline: ${spentDeadline}`);
      }
      await cdp.run({ deadlineAt: Date.now() - 5_000, timeoutIsFatal: false }).catch(() => {});
      if (cdp.timeouts().at(-1) !== 100) {
        throw new Error(`an expired deadline did not fall back to the floor: ${cdp.timeouts().at(-1)}`);
      }
      if (cdp.timeouts()[0] !== 10_000) {
        throw new Error("a deadline-free command lost the lease-fatal timeout");
      }

      // The heartbeat commits without the Runtime.evaluate renderer probe or
      // the overlay refresh while the dialog is pending, and never revokes.
      const heartbeatControl = extractFunction(source, "heartbeatControl");
      const heartbeat = new Function(`
        let controlLease = null;
        let stopCalls = 0, probeCalls = 0, persistCalls = 0, showCalls = 0, verifyCalls = 0;
        function initializeControlState() { return Promise.resolve(); }
        function captureLeaseAuthority(lease) { return { sessionId: lease.sessionId, epoch: lease.epoch }; }
        function verifyDocumentAuthority() { verifyCalls += 1; return Promise.resolve(); }
        function debuggerCommand(_tabId, method) {
          if (method === "Runtime.evaluate") probeCalls += 1;
          return Promise.resolve({});
        }
        function assertLeaseAuthority() {}
        function persistControlState() { persistCalls += 1; return Promise.resolve(); }
        function showControlUi() { showCalls += 1; return Promise.resolve(); }
        function stopControl() { stopCalls += 1; controlLease = null; return Promise.resolve(); }
        ${heartbeatControl}
        return {
          setLease(lease) { controlLease = lease; },
          beat: () => heartbeatControl(),
          state: () => ({ stopCalls, probeCalls, persistCalls, showCalls, verifyCalls, lease: controlLease }),
        };
      `)();
      const lease = {
        sessionId: "lease-1", epoch: 1, tabId: 9,
        expiresAt: Date.now() + 60_000,
        pendingNavigation: null,
        pendingDialog: { type: "confirm" },
        lastHeartbeatAt: 0,
      };
      heartbeat.setLease(lease);
      await heartbeat.beat();
      let state = heartbeat.state();
      if (state.probeCalls !== 0 || state.showCalls !== 0 || state.stopCalls !== 0
        || state.persistCalls !== 1 || state.verifyCalls !== 1 || !state.lease.lastHeartbeatAt) {
        throw new Error(`the dialog heartbeat touched the renderer or revoked: ${JSON.stringify(state)}`);
      }
      lease.pendingDialog = null;
      await heartbeat.beat();
      state = heartbeat.state();
      if (state.probeCalls !== 1 || state.showCalls !== 1 || state.stopCalls !== 0) {
        throw new Error(`the dialog-free heartbeat lost its renderer probe: ${JSON.stringify(state)}`);
      }
      // An authorized navigation stalled behind a beforeunload dialog is not
      // a navigation timeout; without the dialog the stall still revokes.
      lease.pendingNavigation = { authorizedAt: Date.now() - 20_000 };
      lease.pendingDialog = { type: "beforeunload" };
      await heartbeat.beat();
      if (heartbeat.state().stopCalls !== 0) {
        throw new Error("a dialog-stalled navigation was revoked as a timeout");
      }
      lease.pendingDialog = null;
      await heartbeat.beat();
      if (heartbeat.state().stopCalls !== 1) {
        throw new Error("a dialog-free stalled navigation was not revoked");
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run pending-dialog guard harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node pending-dialog guard harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Regression for the defect the live packaged 0.10.0 run exposed: a
/// `confirm()` scheduled by `page.evaluate` opened WHILE the post-action
/// auto-observe was already in flight, the document-identity check at the
/// screenshot-completion boundary failed against the dialog-frozen renderer,
/// and the lease was hard-revoked with `document_changed:screenshot
/// completion`. The 0.10 contract tests missed it because they injected
/// `pendingDialog` before exercising the gate; these harnesses instead open
/// the dialog mid-flight, between the capture call and the completion check.
#[test]
fn a_dialog_that_opens_mid_observation_is_never_paid_for_with_the_lease() {
    let background = extension_source("background.js");

    // The observation reads the dialog record at its own start, because the
    // dispatch gate only proves the state at command entry, and again after
    // the capture, so a snapshot taken across a dialog is discarded instead
    // of published.
    let observe_start = background
        .find("async function observeControlledPage")
        .unwrap();
    let observe_end = background[observe_start..].find("\n}\n").unwrap() + observe_start;
    let observe = &background[observe_start..observe_end];
    assert!(
        observe
            .find("assertDialogNotBlocking(\"observation start\")")
            .unwrap()
            < observe.find("requireControl").unwrap()
    );
    assert!(
        observe.find("captureTab(").unwrap()
            < observe
                .find("assertDialogNotBlocking(\"observation completion\")")
                .unwrap()
    );
    assert!(
        observe
            .find("assertDialogNotBlocking(\"observation completion\")")
            .unwrap()
            < observe.find("return { snapshot, screenshot").unwrap()
    );

    // The overlay boundary revokes too, so it carries the same guard, and the
    // capture teardown does not spend two content timeouts against a frozen
    // renderer before reaching it.
    let ui_start = background
        .find("async function failControlUiClosed")
        .unwrap();
    let ui_end = background[ui_start..].find("\n}\n").unwrap() + ui_start;
    let ui = &background[ui_start..ui_end];
    assert!(ui.find("assertDialogNotBlocking(phase)").unwrap() < ui.find("stopControl").unwrap());
    let capture_start = background.find("async function captureTab").unwrap();
    let capture_end = background[capture_start..].find("\n}\n").unwrap() + capture_start;
    let capture = &background[capture_start..capture_end];
    assert!(capture.contains("const dialogBlocked = Boolean(controlLease?.pendingDialog)"));
    assert!(capture.contains("if (!dialogBlocked) {"));
    assert!(capture.contains("leaseStillActive && !restored && !dialogBlocked"));

    // A dialog-caused refusal is a plain retriable refusal, never laundered
    // into ACTION_OUTCOME_UNKNOWN by the after-dispatch wrapper.
    let after_start = background
        .find("async function verifyDocumentAuthorityAfterDispatch")
        .unwrap();
    let after_end = background[after_start..].find("\n}\n").unwrap() + after_start;
    assert!(background[after_start..after_end].contains("\"BLOCKED_BY_DIALOG\""));

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (start < 0) throw new Error(`missing ${name}`);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const signatureEnd = source.indexOf(") {", start);
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/background.js", "utf8");
      const pendingDialogError = extractFunction(source, "pendingDialogError");
      const assertDialogNotBlocking = extractFunction(source, "assertDialogNotBlocking");
      const failChangedDocument = extractFunction(source, "failChangedDocument");
      const verifyDocumentAuthority = extractFunction(source, "verifyDocumentAuthority");
      const failControlUiClosed = extractFunction(source, "failControlUiClosed");
      const captureTab = extractFunction(source, "captureTab");
      const clearPendingDialog = extractFunction(source, "clearPendingDialog");

      // The real screenshot boundary, driven exactly the way the live run
      // drove it: the capture succeeds, the dialog opens while it is in
      // flight, and the document-identity probe that follows fails against
      // the frozen renderer.
      function newLease() {
        return {
          tabId: 9, sessionId: "lease-1", epoch: 1, documentEpoch: 4,
          loaderId: "L1", frameId: "F1", documentUrl: "https://example.test/",
          navigationReady: true, pendingNavigation: null, pendingDialog: null,
          cursor: { visible: false },
          policy: { allowedHosts: ["example.test"], port: 17373, fullAccess: true },
        };
      }
      const boundary = new Function(`
        const crypto = { randomUUID: () => "capture-1" };
        let controlLease = null;
        let stopCalls = 0, contentCalls = 0, reportedLoaderId = "L1", releaseRetries = 0;
        function retryDialogBlockedInputRelease() { releaseRetries += 1; return Promise.resolve(); }
        let onScreenshot = () => {};
        function requireControl() { return Promise.resolve(controlLease); }
        function captureLeaseAuthority(lease = controlLease) {
          return { tabId: lease.tabId, sessionId: lease.sessionId, epoch: lease.epoch, documentEpoch: lease.documentEpoch };
        }
        function assertLeaseAuthority() {}
        function assertLeaseAuthorityAfterDispatch() {}
        function beginControlCapture() { return ["capture-1"]; }
        function endControlCapture() { return []; }
        function controlUiAcknowledged() { return true; }
        function contentRequest() { contentCalls += 1; return Promise.resolve({}); }
        function showControlUi() { return Promise.resolve({}); }
        function persistControlState() { return Promise.resolve(); }
        function getTab() { return Promise.resolve({ id: 9, url: "https://example.test/" }); }
        function effectiveTabUrl(tab) { return tab.url; }
        function allowedTabVerdict() { return { allowed: true, reason: "" }; }
        function policyUrlVerdict() { return { allowed: true, reason: "" }; }
        function sameDocumentUrl(left, right) { return String(left) === String(right); }
        function stopControl() { stopCalls += 1; controlLease = null; return Promise.resolve(); }
        function debuggerCommand(_tabId, method) {
          if (method === "Page.getFrameTree") {
            return Promise.resolve({ frameTree: { frame: { id: "F1", loaderId: reportedLoaderId, url: "https://example.test/" } } });
          }
          if (method === "Page.captureScreenshot") {
            onScreenshot();
            return Promise.resolve({ data: "AAAA" });
          }
          return Promise.resolve({});
        }
        ${pendingDialogError}
        ${assertDialogNotBlocking}
        ${failChangedDocument}
        ${verifyDocumentAuthority}
        ${failControlUiClosed}
        ${captureTab}
        ${clearPendingDialog}
        return {
          reset(lease) { controlLease = lease; stopCalls = 0; contentCalls = 0; reportedLoaderId = "L1"; releaseRetries = 0; onScreenshot = () => {}; },
          duringScreenshot(hook) { onScreenshot = hook; },
          navigateAway() { reportedLoaderId = "L2"; },
          openDialog(type) { controlLease.pendingDialog = { type }; },
          capture: () => captureTab({ id: 9, url: "https://example.test/" }, null, null),
          verify: (label) => verifyDocumentAuthority(9, captureLeaseAuthority(), null, label),
          clear: () => clearPendingDialog(captureLeaseAuthority()),
          lease: () => controlLease,
          stops: () => stopCalls,
          contents: () => contentCalls,
          releaseRetries: () => releaseRetries,
        };
      `)();

      // 1. The live ordering. The dialog opens AFTER the capture began, so
      //    the completion check is the first boundary that sees it.
      const raced = newLease();
      boundary.reset(raced);
      boundary.duringScreenshot(() => {
        boundary.openDialog("confirm");
        boundary.navigateAway();
      });
      let racedFailure = null;
      try { await boundary.capture(); } catch (error) { racedFailure = error; }
      if (racedFailure?.code !== "BLOCKED_BY_DIALOG") {
        throw new Error(`a dialog opened mid-capture was misreported as ${racedFailure && racedFailure.code}`);
      }
      if (boundary.stops() !== 0 || boundary.lease() !== raced || !raced.pendingDialog) {
        throw new Error("a dialog opened mid-capture revoked the lease");
      }
      // The frozen renderer is not asked to acknowledge the teardown either:
      // only the capture-begin message was ever sent.
      if (boundary.contents() !== 1) {
        throw new Error(`the capture teardown talked to a dialog-frozen renderer ${boundary.contents()} times`);
      }

      // 2. The same identity failure with NO dialog still revokes: the
      //    fail-closed guarantee is untouched.
      const changed = newLease();
      boundary.reset(changed);
      boundary.duringScreenshot(() => boundary.navigateAway());
      let changedFailure = null;
      try { await boundary.capture(); } catch (error) { changedFailure = error; }
      if (changedFailure?.code !== "DOCUMENT_CHANGED" || boundary.stops() !== 1 || boundary.lease() !== null) {
        throw new Error(`a real document change stopped revoking: ${changedFailure && changedFailure.code}`);
      }

      // 3. A document that really changed while the dialog was open is not
      //    forgiven: once the dialog is resolved the next check runs in full
      //    and revokes.
      const forgiven = newLease();
      boundary.reset(forgiven);
      boundary.openDialog("confirm");
      boundary.navigateAway();
      let blockedCheck = null;
      try { await boundary.verify("observation snapshot"); } catch (error) { blockedCheck = error; }
      if (blockedCheck?.code !== "BLOCKED_BY_DIALOG" || boundary.stops() !== 0 || boundary.lease() !== forgiven) {
        throw new Error("a document check under a dialog revoked the lease");
      }
      if (await boundary.clear() !== true || forgiven.pendingDialog !== null) {
        throw new Error("page.handleDialog did not clear the pending dialog record");
      }
      // Clearing the record is also the moment a release the dialog blocked
      // becomes possible again, so the retry runs from this one choke point.
      if (boundary.releaseRetries() !== 1) {
        throw new Error("clearing a dialog did not retry the input release it blocked");
      }
      let afterDialog = null;
      try { await boundary.verify("observation snapshot"); } catch (error) { afterDialog = error; }
      if (afterDialog?.code !== "DOCUMENT_CHANGED" || boundary.stops() !== 1 || boundary.lease() !== null) {
        throw new Error(`a navigation hidden by a dialog was forgiven after it closed: ${afterDialog && afterDialog.code}`);
      }

      // 4. The observation path itself: a dialog at the start refuses before
      //    touching the renderer, a dialog that opens inside the capture
      //    discards the observation, and a dialog-free run still publishes.
      const observeControlledPage = extractFunction(source, "observeControlledPage");
      const observation = new Function(`
        let controlLease = null;
        let frameSnapshot = null, frameTreeRevision = 0;
        const frameSkips = [];
        const attachedFrames = new Map();
        let stopCalls = 0, requireCalls = 0, snapshotCalls = 0, captureCalls = 0;
        let onCapture = () => {};
        function requireControl() { requireCalls += 1; return Promise.resolve(controlLease); }
        function captureLeaseAuthority(lease = controlLease) {
          return { tabId: lease.tabId, sessionId: lease.sessionId, epoch: lease.epoch, documentEpoch: lease.documentEpoch };
        }
        function assertLeaseAuthority() {}
        function verifyDocumentAuthority() { return Promise.resolve({}); }
        function showControlUi() { return Promise.resolve({}); }
        function persistControlState() { return Promise.resolve(); }
        function contentRequest() {
          snapshotCalls += 1;
          return Promise.resolve({ generation: "g1", viewport: { width: 800, height: 600 }, elements: [{ ref: "g1.e1" }], frameOwners: [] });
        }
        function collectFrameObservations(_tabId, snapshot) {
          return Promise.resolve({ elements: snapshot.elements, frames: [], frameSummary: { supported: false } });
        }
        function mergeFrameObservations(snapshot) { return { elements: snapshot.elements, frames: [], frameSummary: { supported: false } }; }
        function frameObservationIsSilent() { return true; }
        function rethrowLeaseFatalFrameError() {}
        function classifyRisk() { return "low"; }
        function publicControlState() { return { active: true }; }
        function stopControl() { stopCalls += 1; controlLease = null; return Promise.resolve(); }
        function captureTab() { captureCalls += 1; onCapture(); return Promise.resolve("data:image/jpeg;base64,AAAA"); }
        ${pendingDialogError}
        ${assertDialogNotBlocking}
        ${observeControlledPage}
        return {
          reset(lease) { controlLease = lease; stopCalls = 0; requireCalls = 0; snapshotCalls = 0; captureCalls = 0; onCapture = () => {}; },
          duringCapture(hook) { onCapture = hook; },
          openDialog(type) { controlLease.pendingDialog = { type }; },
          observe: () => observeControlledPage({ id: 9, url: "https://example.test/" }, null),
          lease: () => controlLease,
          state: () => ({ stopCalls, requireCalls, snapshotCalls, captureCalls }),
        };
      `)();

      const midObservation = newLease();
      observation.reset(midObservation);
      observation.duringCapture(() => observation.openDialog("confirm"));
      let discarded = null;
      try { await observation.observe(); } catch (error) { discarded = error; }
      if (discarded?.code !== "BLOCKED_BY_DIALOG") {
        throw new Error(`an observation raced by a dialog was published or misreported: ${discarded && discarded.code}`);
      }
      if (observation.state().stopCalls !== 0 || observation.lease() !== midObservation) {
        throw new Error("an observation raced by a dialog revoked the lease");
      }

      const alreadyOpen = newLease();
      alreadyOpen.pendingDialog = { type: "alert" };
      observation.reset(alreadyOpen);
      let refusedEarly = null;
      try { await observation.observe(); } catch (error) { refusedEarly = error; }
      const earlyState = observation.state();
      if (refusedEarly?.code !== "BLOCKED_BY_DIALOG"
        || earlyState.requireCalls !== 0 || earlyState.snapshotCalls !== 0 || earlyState.captureCalls !== 0) {
        throw new Error(`an observation under an open dialog touched the renderer: ${JSON.stringify(earlyState)}`);
      }

      const healthy = newLease();
      observation.reset(healthy);
      const published = await observation.observe();
      if (!published?.snapshot || !published.screenshot || published.snapshot.elements[0].risk !== "low") {
        throw new Error("the dialog guards broke the ordinary observation");
      }
      if (observation.state().stopCalls !== 0 || observation.lease() !== healthy) {
        throw new Error("an ordinary observation revoked the lease");
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run mid-observation dialog harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node mid-observation dialog harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The two revocation routes the 0.11 dialog guard did not cover, both of
/// them reachable on the commonest real dialog of all: `alert()` or
/// `confirm()` inside the handler of the element the agent just clicked.
/// One is the held-input cleanup that runs in `trustedClick`'s `finally`
/// against the dialog-frozen renderer; the other is the worker-restart
/// recovery, whose catch-all used to swallow the guard's own
/// `BLOCKED_BY_DIALOG` and revoke anyway.
#[test]
fn a_dialog_revokes_the_lease_through_neither_input_release_nor_worker_recovery() {
    let background = extension_source("background.js");

    // Both release cleanups consult the dialog record immediately before
    // revoking, exactly like every other revocation boundary.
    for (function, reason) in [
        (
            "async function releaseHeldMouseInput",
            "mouse_release_failed",
        ),
        ("async function releaseHeldKeyInput", "key_release_failed"),
    ] {
        let start = background.find(function).unwrap();
        let end = background[start..].find("\n}\n").unwrap() + start;
        let body = &background[start..end];
        assert!(
            body.find("assertDialogNotBlocking").unwrap()
                < body.find(&format!("stopControl(\"{reason}\"")).unwrap(),
            "{function} can still revoke a lease under a pending dialog"
        );
    }

    // A deferred release is not forgiven permanently: both places a dialog
    // can end retry it the moment the renderer can answer again.
    let clear_start = background
        .find("async function clearPendingDialog")
        .unwrap();
    let clear_end = background[clear_start..].find("\n}\n").unwrap() + clear_start;
    assert!(background[clear_start..clear_end].contains("retryDialogBlockedInputRelease"));
    let closed_start = background
        .find("if (method === \"Page.javascriptDialogClosed\")")
        .unwrap();
    let closed_end = background[closed_start..].find("\n  }\n").unwrap() + closed_start;
    assert!(background[closed_start..closed_end].contains("retryDialogBlockedInputRelease"));

    // The worker-restart recovery no longer swallows BLOCKED_BY_DIALOG into
    // its catch-all revocation.
    let recovery_start = background
        .find("async function finishRecoveredLease")
        .unwrap();
    let recovery_end = background[recovery_start..].find("\n}\n").unwrap() + recovery_start;
    let recovery = &background[recovery_start..recovery_end];
    assert!(
        recovery
            .find("error?.code === \"BLOCKED_BY_DIALOG\"")
            .unwrap()
            < recovery
                .find("stopControl(\"recovered_document_unverified\"")
                .unwrap()
    );

    let script = r#"
      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (start < 0) throw new Error(`missing ${name}`);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const signatureEnd = source.indexOf(") {", start);
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const source = fs.readFileSync("extension/background.js", "utf8");
      const pendingDialogError = extractFunction(source, "pendingDialogError");
      const assertDialogNotBlocking = extractFunction(source, "assertDialogNotBlocking");
      const releaseHeldMouseInput = extractFunction(source, "releaseHeldMouseInput");
      const releaseHeldKeyInput = extractFunction(source, "releaseHeldKeyInput");
      const retryDialogBlockedInputRelease = extractFunction(source, "retryDialogBlockedInputRelease");

      // The real cleanup functions, driven the way trustedClick's finally
      // drives them: the renderer is frozen, so the release command is never
      // acknowledged.
      const release = new Function(`
        let controlLease = null;
        let stopCalls = 0, dispatched = 0, frozen = false;
        const heldMouseInputs = new Map();
        const heldKeyInputs = new Map();
        function bestEffortDebuggerRelease() {
          dispatched += 1;
          return Promise.resolve(!controlLease?.pendingDialog && !frozen);
        }
        function clearHeldInputIntent(map, key) { map.delete(key); return Promise.resolve(); }
        function stopControl() { stopCalls += 1; controlLease = null; return Promise.resolve(); }
        ${pendingDialogError}
        ${assertDialogNotBlocking}
        ${releaseHeldMouseInput}
        ${releaseHeldKeyInput}
        ${retryDialogBlockedInputRelease}
        return {
          setLease(lease) { controlLease = lease; },
          freeze(value) { frozen = value; },
          hold(mouseKey, keyKey) {
            heldMouseInputs.set(mouseKey, { tabId: 9, releaseMethod: "Input.dispatchMouseEvent", releaseParams: { type: "mouseReleased" } });
            heldKeyInputs.set(keyKey, { tabId: 9, releaseMethod: "Input.dispatchKeyEvent", releaseParams: { type: "keyUp" } });
          },
          releaseMouse: (key) => releaseHeldMouseInput(key, heldMouseInputs.get(key)),
          releaseKey: (key) => releaseHeldKeyInput(key, heldKeyInputs.get(key)),
          retry: (tabId) => retryDialogBlockedInputRelease(tabId),
          held: () => heldMouseInputs.size + heldKeyInputs.size,
          stops: () => stopCalls,
          dispatches: () => dispatched,
          lease: () => controlLease,
        };
      `)();

      // 1. alert() inside the click handler: the press and its release both
      //    stall on the frozen renderer, so the finally cleanup runs under
      //    the dialog. It reports the dialog and keeps the lease.
      const clicked = { tabId: 9, sessionId: "lease-1", epoch: 1, pendingDialog: { type: "confirm" } };
      release.setLease(clicked);
      release.hold("mouse-1", "key-1");
      let mouseBlocked = null;
      try { await release.releaseMouse("mouse-1"); } catch (error) { mouseBlocked = error; }
      let keyBlocked = null;
      try { await release.releaseKey("key-1"); } catch (error) { keyBlocked = error; }
      if (mouseBlocked?.code !== "BLOCKED_BY_DIALOG" || keyBlocked?.code !== "BLOCKED_BY_DIALOG") {
        throw new Error(`a dialog-blocked release was misreported as ${mouseBlocked && mouseBlocked.code}/${keyBlocked && keyBlocked.code}`);
      }
      if (release.stops() !== 0 || release.lease() !== clicked) {
        throw new Error("a dialog-blocked input release revoked the lease");
      }
      if (release.held() !== 2) {
        throw new Error("a dialog-blocked release dropped the durable intent it still owes");
      }

      // 2. Nothing is retried while the dialog is still up: the deferred
      //    release waits instead of burning the frozen renderer.
      const dispatchedBeforeRetry = release.dispatches();
      await release.retry(9);
      if (release.dispatches() !== dispatchedBeforeRetry || release.held() !== 2) {
        throw new Error("a retry ran against a renderer that was still frozen");
      }

      // 3. The dialog is resolved, so the owed releases go out and settle.
      clicked.pendingDialog = null;
      await release.retry(9);
      if (release.held() !== 0 || release.stops() !== 0 || release.lease() !== clicked) {
        throw new Error("the post-dialog retry did not release the inputs it deferred");
      }

      // 4. The fail-closed rule is untouched: a release that stays
      //    unacknowledged with NO dialog pending still revokes.
      release.hold("mouse-2", "key-2");
      release.freeze(true);
      let unacknowledged = null;
      try { await release.releaseMouse("mouse-2"); } catch (error) { unacknowledged = error; }
      if (unacknowledged?.code !== "INPUT_RELEASE_FAILED" || release.stops() !== 1 || release.lease() !== null) {
        throw new Error(`a dialog-free release failure stopped revoking: ${unacknowledged && unacknowledged.code}`);
      }

      // The worker-restart recovery: the persisted lease carries its
      // pendingDialog, the browser-side document identity still verifies, and
      // only the overlay repaint is impossible.
      const failControlUiClosed = extractFunction(source, "failControlUiClosed");
      const finishRecoveredLease = extractFunction(source, "finishRecoveredLease");
      const recovery = new Function(`
        let controlLease = null;
        let stopCalls = 0, persistCalls = 0, heartbeatCalls = 0, identityBlocked = false;
        function initializeLeaseDocument(lease) {
          if (identityBlocked) return Promise.reject(pendingDialogError("recovered document identity"));
          lease.navigationReady = true;
          return Promise.resolve({});
        }
        function persistControlState() { persistCalls += 1; return Promise.resolve(); }
        function scheduleHeartbeat() { heartbeatCalls += 1; }
        function stopControl() { stopCalls += 1; controlLease = null; return Promise.resolve(); }
        function showControlUi() {
          return failControlUiClosed(controlLease, "show", new Error("the page did not confirm a painted control indicator"));
        }
        ${pendingDialogError}
        ${assertDialogNotBlocking}
        ${failControlUiClosed}
        ${finishRecoveredLease}
        return {
          reset(lease, blocked) { controlLease = lease; stopCalls = 0; persistCalls = 0; heartbeatCalls = 0; identityBlocked = blocked === true; },
          finish: () => finishRecoveredLease({ id: 9 }, { allowedHosts: [], port: 17373, fullAccess: true }),
          lease: () => controlLease,
          state: () => ({ stopCalls, persistCalls, heartbeatCalls }),
        };
      `)();

      // 5. Restart with the dialog still open: the lease survives and keeps
      //    its heartbeat, and the indicator is repainted after the dialog.
      const restarted = { tabId: 9, sessionId: "lease-2", epoch: 2, navigationReady: false, pendingDialog: { type: "alert" } };
      recovery.reset(restarted, false);
      if (await recovery.finish() !== true || recovery.lease() !== restarted) {
        throw new Error("a worker restart under a dialog revoked the recovered lease");
      }
      if (recovery.state().stopCalls !== 0 || recovery.state().heartbeatCalls < 1) {
        throw new Error(`the dialog-blocked recovery revoked or stopped watching: ${JSON.stringify(recovery.state())}`);
      }

      // 6. The same unacknowledged indicator with no dialog still revokes.
      const dark = { tabId: 9, sessionId: "lease-3", epoch: 3, navigationReady: false, pendingDialog: null };
      recovery.reset(dark, false);
      if (await recovery.finish() !== false || recovery.lease() !== null || recovery.state().stopCalls < 1) {
        throw new Error("a dialog-free unacknowledged indicator survived a recovery");
      }

      // 7. And a recovery that could not verify the document at all is the
      //    stated boundary: it still revokes, dialog or not.
      const unverified = { tabId: 9, sessionId: "lease-4", epoch: 4, navigationReady: false, pendingDialog: { type: "prompt" } };
      recovery.reset(unverified, true);
      if (await recovery.finish() !== false || recovery.lease() !== null || recovery.state().stopCalls !== 1) {
        throw new Error("an unverifiable recovered document was kept behind a dialog");
      }
    "#;
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run dialog release/recovery harness: {error}"),
    };
    assert!(
        output.status.success(),
        "Node dialog release/recovery harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn server_and_extension_command_allowlists_match() {
    let background = extension_source("background.js");
    let start = background.find("const COMMANDS = new Set([").unwrap();
    let end = background[start..].find("]);").unwrap() + start;
    let extension_commands = quoted_strings(&background[start..end]);
    let server_commands = ACTION_METHODS
        .iter()
        .map(|method| (*method).to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(extension_commands, server_commands);
}

#[test]
fn distributed_text_contains_no_hangul() {
    fn inspect(path: &Path) {
        for entry in fs::read_dir(path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.is_dir() {
                inspect(&path);
            } else if let Ok(source) = fs::read_to_string(&path) {
                assert!(
                    !source.chars().any(|character| {
                        matches!(character as u32, 0x1100..=0x11ff | 0x3130..=0x318f | 0xac00..=0xd7af)
                    }),
                    "Hangul found in {}",
                    path.display()
                );
            }
        }
    }

    for root in ["extension", "public", "src", "tests", "docs"] {
        inspect(Path::new(root));
    }
    for file in ["README.md", "SECURITY.md", "AGENTS.md"] {
        inspect_file(Path::new(file));
    }
}

fn inspect_file(path: &Path) {
    let source = fs::read_to_string(path).unwrap();
    assert!(
        !source.chars().any(|character| {
            matches!(character as u32, 0x1100..=0x11ff | 0x3130..=0x318f | 0xac00..=0xd7af)
        }),
        "Hangul found in {}",
        path.display()
    );
}

// ---------------------------------------------------------------------------
// v0.11 cross-origin frame (OOPIF) contracts
// ---------------------------------------------------------------------------

/// The only failures a frame may carry out of a read-only observation: each
/// one invalidates the whole observation, not just one frame's contribution.
/// Frame latency is deliberately absent.
const FRAME_LEASE_FATAL_CODES: &[&str] = &[
    "CONTROL_CANCELED",
    "COMMAND_CANCELED",
    "DOCUMENT_CHANGED",
    "ACTION_OUTCOME_UNKNOWN",
    "CDP_OUTCOME_UNKNOWN",
    "BLOCKED_BY_DIALOG",
];

#[test]
fn frame_targets_auto_attach_is_flat_bounded_and_iframe_only() {
    let background = extension_source("background.js");
    assert!(background.contains("const FRAME_MAX_ATTACHED = 16"));
    assert!(background.contains("const FRAME_MAX_DEPTH = 5"));
    assert!(background.contains("const FRAME_PER_FRAME_ELEMENT_CAP = 120"));
    assert!(background.contains("const FRAME_TOP_ELEMENT_CAP = 400"));
    assert!(background.contains("const FRAME_ELEMENT_CAP_TOTAL = 500"));
    assert!(background.contains("const FRAME_SKIP_REPORT_MAX = 32"));
    assert!(background.contains("const FRAME_OFFSET_TOLERANCE_PX = 2"));
    assert!(background.contains("const FRAME_OBSERVE_BUDGET_MS = 4_000"));
    assert!(background.contains("const FRAME_OBSERVE_MIN_TIMEOUT_MS = 100"));

    // Read-only frame observation may cost the frame, never the lease: the
    // non-fatal timeout branch has to sit ahead of the revocation.
    let non_fatal = background
        .find("if (options?.timeoutIsFatal === false) throw frameObserveTimeoutError")
        .expect("frame observation timeouts are lease-fatal again");
    let revocation = background
        .find("await stopControl(`cdp_timeout:${method}`")
        .unwrap();
    assert!(
        non_fatal < revocation,
        "a frame observation timeout can still revoke the lease"
    );
    assert!(!FRAME_LEASE_FATAL_CODES.contains(&"FRAME_OBSERVE_TIMEOUT"));
    let fatal_start = background
        .find("function rethrowLeaseFatalFrameError")
        .unwrap();
    let fatal_end = background[fatal_start..].find("\n}\n").unwrap() + fatal_start;
    for code in FRAME_LEASE_FATAL_CODES {
        assert!(
            background[fatal_start..fatal_end].contains(code),
            "{code} no longer propagates out of the frame pass"
        );
    }
    assert!(!background[fatal_start..fatal_end].contains("FRAME_OBSERVE_TIMEOUT"));

    let enable_start = background
        .find("async function enableFrameAutoAttach")
        .unwrap();
    let enable_end = background[enable_start..]
        .find("\nasync function probeFrameSession")
        .unwrap()
        + enable_start;
    let enable = &background[enable_start..enable_end];
    assert!(enable.contains("\"Target.setAutoAttach\""));
    assert!(enable.contains("autoAttach: true"));
    assert!(enable.contains("waitForDebuggerOnStart: false"));
    assert!(enable.contains("flatten: true"));

    // Any auto-attached relative that is not an iframe of the leased tab is
    // detached immediately, so the lease never widens past the page target
    // plus its own frames.
    let record_start = background
        .find("async function recordAttachedTarget")
        .unwrap();
    let record_end = background[record_start..]
        .find("\n// Never fails the lease")
        .unwrap()
        + record_start;
    let record = &background[record_start..record_end];
    assert!(record.contains("targetInfo.type !== \"iframe\""));
    assert!(record.contains("detachFrameTarget(tabId, sessionId, \"non_iframe_target\")"));
    assert!(record.contains("detachFrameTarget(tabId, sessionId, \"no_lease\")"));
    assert!(record.contains("attachedFrames.size >= FRAME_MAX_ATTACHED"));
    assert!(record.contains("parentKey === String(sessionId)"));
    assert!(record.contains("parent.depth + 1"));
    assert!(record.contains("depth > FRAME_MAX_DEPTH"));
    assert!(record.contains("descendantAutoAttachEnabled: false"));
    assert!(record.contains("\"budget_frames\""));
    assert!(record.contains("\"blank_document\""));
    assert!(background.contains("\"Target.detachFromTarget\""));

    let recursive_start = background
        .find("async function enableDescendantFrameAutoAttach")
        .unwrap();
    let recursive_end = background[recursive_start..]
        .find("\n// Never fails the lease")
        .unwrap()
        + recursive_start;
    let recursive = &background[recursive_start..recursive_end];
    assert!(recursive.contains("record.depth > FRAME_MAX_DEPTH"));
    assert!(recursive.contains("\"Target.setAutoAttach\""));
    assert!(recursive.contains("record.sessionId"));
    assert!(recursive.contains("options"));

    // The routing probe must stay discriminating: a truthiness check would
    // accept the ROOT frame tree from a route that strips sessionId.
    let probe_start = background.find("async function probeFrameSession").unwrap();
    let probe_end = background[probe_start..]
        .find("\nasync function prepareOwnerSession")
        .unwrap()
        + probe_start;
    let probe = &background[probe_start..probe_end];
    assert!(probe.contains("frame.id !== record.targetId"));
    assert!(probe.contains("enableDescendantFrameAutoAttach"));
    assert!(probe.contains("rethrowLeaseFatalFrameError(error)"));
    assert!(probe.contains("frameSkipReasonFor(error, \"session_probe_failed\")"));
    assert!(probe.contains("attachedFrames.get(record.sessionId) !== record"));
    assert!(probe.contains("throw frameDetachedError(\"frame session probing\")"));
    assert!(background.contains("while (true) {"));
    assert!(background.contains("!visited.has(candidate.sessionId)"));
    assert!(background.contains("Date.now() >= options.deadlineAt"));
    assert!(background.contains("session_routing_unverified"));
    assert_eq!(manifest()["minimum_chrome_version"], "140");
}

#[test]
fn child_debugger_events_route_only_nested_target_lifecycle() {
    let background = extension_source("background.js");
    let listener_start = background
        .find("chrome.debugger.onEvent.addListener(")
        .unwrap();
    let listener_end = background[listener_start..]
        .find("chrome.tabs.onUpdated.addListener(")
        .unwrap()
        + listener_start;
    let listener = &background[listener_start..listener_end];
    let gate = listener.find("if (source.sessionId) {").unwrap();
    assert!(listener[gate..].contains("handleFrameSessionEvent(source, method, params);"));
    // Every existing root-only branch must sit after the gate: an
    // out-of-process iframe's own Page.frameNavigated carries no parentId and
    // would otherwise be read as a top-level navigation.
    for branch in [
        "method === \"Page.frameNavigated\"",
        "method === \"Page.navigatedWithinDocument\"",
        "method === \"Page.javascriptDialogOpening\"",
        "method === \"Page.javascriptDialogClosed\"",
    ] {
        let at = listener
            .find(branch)
            .unwrap_or_else(|| panic!("missing {branch}"));
        assert!(
            at > gate,
            "{branch} is not gated behind the child-session return"
        );
    }
    assert_eq!(listener.matches("handleFrameSessionEvent").count(), 1);
    assert!(listener.contains("method === \"Target.attachedToTarget\""));
    assert!(listener.contains("method === \"Target.detachedFromTarget\""));

    let handler_start = background.find("function handleFrameSessionEvent").unwrap();
    let handler_end = background[handler_start..]
        .find("\nconst POINTER_CANDIDATE_COUNT")
        .unwrap()
        + handler_start;
    let handler = &background[handler_start..handler_end];
    let nested_attach = handler
        .find("method === \"Target.attachedToTarget\"")
        .unwrap();
    let nested_detach = handler
        .find("method === \"Target.detachedFromTarget\"")
        .unwrap();
    let parent_lookup = handler
        .find("const record = attachedFrames.get(source.sessionId)")
        .unwrap();
    assert!(nested_attach < parent_lookup);
    assert!(nested_detach < parent_lookup);
    assert!(handler.contains(
        "recordAttachedTarget(source.tabId, params?.sessionId, params?.targetInfo, source.sessionId)"
    ));
    assert!(handler.contains("dropFrameTargetTree(params?.sessionId)"));
}

#[test]
fn frame_sessions_are_cleared_on_every_lease_teardown() {
    let background = extension_source("background.js");
    // One choke point: stopControl and hardRevokeDetached both pass through
    // synchronouslyTakeControlLease, and only clearFrameSessions empties the
    // session map for a teardown.
    assert_eq!(background.matches("clearFrameSessions()").count(), 2);
    let take_start = background
        .find("function synchronouslyTakeControlLease")
        .unwrap();
    let take_end = background[take_start..]
        .find("\nasync function bestEffortDebuggerRelease")
        .unwrap()
        + take_start;
    assert!(background[take_start..take_end].contains("clearFrameSessions()"));

    let clear_start = background.find("function clearFrameSessions()").unwrap();
    let clear_end = background[clear_start..].find("\n}\n").unwrap() + clear_start;
    let clear = &background[clear_start..clear_end];
    for cleared in [
        "attachedFrames.clear()",
        "frameParents.clear()",
        "frameSkips.length = 0",
        "frameSnapshot = null",
        "rootWorldContextId = null",
        "reason: \"no_lease\"",
    ] {
        assert!(clear.contains(cleared), "teardown does not clear {cleared}");
    }
    // No second, unverified debugger teardown path was added.
    assert_eq!(background.matches("chrome.debugger.detach(").count(), 1);
}

#[test]
fn frame_agent_is_read_only_and_isolated() {
    let agent = extension_source("frame-agent.js");
    let core = extension_source("dom-core.js");
    let background = extension_source("background.js");
    for forbidden in [
        ".click(",
        ".focus(",
        "dispatchEvent",
        "Input.",
        "chrome.",
        "setNativeValue",
        "innerHTML",
    ] {
        assert!(
            !agent.contains(forbidden),
            "frame agent contains {forbidden}"
        );
        assert!(!core.contains(forbidden), "dom core contains {forbidden}");
    }
    // The agent only ever runs in a dedicated isolated world, never the main
    // world where a page could redefine the proof primitives.
    assert_eq!(
        background.matches("\"Page.createIsolatedWorld\"").count(),
        3
    );
    assert_eq!(background.matches("grantUniveralAccess: false").count(), 3);
    assert!(background.contains("worldName: FRAME_AGENT_WORLD_NAME"));
    assert!(background.contains("const FRAME_AGENT_WORLD_NAME = \"__lbb_frame_agent__\""));
    assert!(!background.contains("grantUniveralAccess: true"));

    // Lease-keyed: a leftover agent from an older lease refuses everything.
    assert!(
        agent.contains("FRAME_AGENT_STALE: this frame agent belongs to an older control lease")
    );
    assert!(agent.contains("function call(request)"));
    assert!(agent.contains("return { ok: true, result: run(request ?? {}) };"));
}

#[test]
fn frame_agent_and_content_share_one_dom_core() {
    let core = extension_source("dom-core.js");
    let sources = [
        ("extension/background.js", extension_source("background.js")),
        ("extension/content.js", extension_source("content.js")),
        ("extension/dom-core.js", core.clone()),
        (
            "extension/frame-agent.js",
            extension_source("frame-agent.js"),
        ),
    ];
    for name in [
        "clean",
        "normalizedFieldIdentifier",
        "isSensitiveFieldMetadata",
        "visible",
        "labelledBy",
        "accessibleName",
        "roleOf",
        "boundsOf",
        "describe",
        "targetSignature",
        "sameBounds",
        "composedContains",
        "deepElementFromPoint",
        "composedCandidates",
        "pointTarget",
        "validateRecord",
        "compareProof",
        "createRevisionTracker",
    ] {
        let declaration = format!("function {name}(");
        let total: usize = sources
            .iter()
            .map(|(_, source)| source.matches(&declaration).count())
            .sum();
        assert_eq!(total, 1, "{name} is not declared exactly once");
        assert_eq!(
            core.matches(&declaration).count(),
            1,
            "{name} is not declared in extension/dom-core.js"
        );
    }
    // The content script consumes the core instead of re-implementing it.
    let content = &sources[1].1;
    assert!(content.contains("globalThis.__LBB_DOM_CORE__({ isExcludedNode: isControlNode })"));
    assert!(content.contains("core.createRevisionTracker({"));
    // The content script no longer owns any of the moved core constants.
    assert!(!content.contains("GEOMETRY_TOLERANCE_PX"));
    assert!(!content.contains("candidateSelector"));
    let background = &sources[0].1;
    assert!(background.contains("const CONTENT_SCRIPT_FILES = [\"dom-core.js\", \"content.js\"]"));
    assert!(background.contains("files: CONTENT_SCRIPT_FILES"));
    assert!(!background.contains("files: [\"content.js\"]"));
    // The evaluated frame source is rebuilt from the packaged core, so there
    // is no second transcription of these bodies anywhere.
    assert!(core.contains("globalThis.__LBB_DOM_CORE_SOURCE__"));
    assert!(
        sources[3]
            .1
            .contains("globalThis.__LBB_FRAME_AGENT_SOURCE__")
    );
    assert!(background.contains("globalThis.__LBB_DOM_CORE_SOURCE__()"));
}

#[test]
fn frame_refs_are_refused_by_every_action_except_click_and_hover() {
    let background = extension_source("background.js");
    let content = extension_source("content.js");
    for method in ["page.fill", "page.select"] {
        let start = background.find(&format!("case \"{method}\"")).unwrap();
        let end = background[start + 5..]
            .find("\n    case \"")
            .map(|end| start + 5 + end)
            .unwrap_or(background.len());
        let body = &background[start..end];
        let guard = body
            .find(&format!("assertTopLevelRef(params.ref, \"{method}\")"))
            .unwrap_or_else(|| panic!("{method} does not refuse frame refs"));
        assert!(
            guard < body.find("contentRequest(").unwrap(),
            "{method} refuses a frame ref only after a content request"
        );
    }
    assert!(background.contains("FRAME_ACTION_UNSUPPORTED:"));
    assert!(background.contains("only page.click and page.hover accept frame-scoped refs"));
    // A frame ref that somehow reaches the top document fails loudly instead
    // of resolving against a colliding top-frame element key.
    assert!(content.contains("FRAME_REF_MISROUTED: this ref belongs to a subframe"));
    // Only the pointer paths route into a frame.
    assert_eq!(background.matches("prepareFrameTarget(").count(), 3);

    // A page with no frames at all publishes no frame keys, so its
    // observation stays byte-identical to a pre-frame observation.
    assert!(background.contains("if (!frameObservationIsSilent(merged)) {"));
    let silent_start = background
        .find("function frameObservationIsSilent(merged)")
        .unwrap();
    let silent_end = background[silent_start..].find("\n}\n").unwrap() + silent_start;
    let silent = &background[silent_start..silent_end];
    for condition in [
        "merged.frames.length === 0",
        "summary.ownersSeen === 0",
        "summary.attached === 0",
        "summary.skipped.length === 0",
    ] {
        assert!(silent.contains(condition), "silence ignores {condition}");
    }
}

/// Shared Node harness that runs the real frame functions extracted from
/// `extension/background.js` against a stubbed Chrome DevTools Protocol.
const FRAME_HARNESS_PRELUDE: &str = r##"      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (start < 0) throw new Error(`missing ${name}`);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const signatureEnd = source.indexOf(") {", source.indexOf(marker));
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const background = fs.readFileSync("extension/background.js", "utf8");
      const frameFunctions = [
        "frameOriginOf", "frameDetachedError", "staleFrameTreeError",
        "frameObserveTimeoutError", "frameObserveOptions", "frameSkipReasonFor",
        "rethrowLeaseFatalFrameError", "recordFrameSkip", "markFrameTreeChanged", "dropFrameTargetTree",
        "clearFrameSessions", "assertFrameSnapshotFresh", "parseFrameRef",
        "assertTopLevelRef", "resolveFrameRecord", "quadRect", "ownerContentOrigin",
        "frameOwnerBoxShifted",
        "accumulateFrameOffset", "translateBounds", "intersects",
        "frameElementInViewport", "frameBoxOf", "frameAncestors",
        "frameOwnerWorldContextId", "frameDepthOf", "collectTreeFrames",
        "orderFrameRecords", "mergeFrameObservations", "loadFrameAgentSource",
        "detachFrameTarget", "recordAttachedTarget", "enableDescendantFrameAutoAttach", "enableFrameAutoAttach",
        "probeFrameSession", "prepareOwnerSession", "measureFrameOwner",
        "installFrameAgent", "frameContentRequest", "verifyFrameAuthority", "resolveOwnerWorld",
        "verifyFrameOwnerHitTest", "observeFrame", "collectFrameObservations",
        "prepareFrameTarget", "frameClickHooks", "handleFrameSessionEvent",
        "assertPointerArrival", "trustedClick",
      ].map((name) => extractFunction(background, name)).join("\n");

      // One frame-local rectangle per frame, so a translated bound is only
      // ever the frame agent's own measurement plus the accumulated offset.
      function defaultWorld(overrides = {}) {
        return {
          routeStripsSession: false,
          ownerHit: true,
          rootTree: {
            frame: { id: "ROOT", loaderId: "L0", url: "https://top.example/" },
            childFrames: [{ frame: { id: "F1", parentId: "ROOT", url: "https://pay.example/" } }],
          },
          frames: {
            "child-1": {
              targetId: "F1",
              parentSession: null,
              tree: { frame: { id: "F1", parentId: "ROOT", loaderId: "L1", url: "https://pay.example/" } },
              owner: { backendNodeId: 501, x: 120, y: 64, width: 380, height: 220 },
              elements: [{
                key: "e1", role: "button", name: "Pay", type: "submit", disabled: false,
                sensitive: false, inViewport: true, bounds: { x: 5, y: 5, width: 40, height: 20 },
                proof: { signature: "sig-1", bounds: { x: 5, y: 5, width: 40, height: 20 } },
              }],
            },
          },
          ...overrides,
        };
      }

      function makeFrameBridge(world = defaultWorld(), options = {}) {
        const calls = [];
        const sessionByFrameId = () => {
          const map = new Map();
          for (const [sessionId, frame] of Object.entries(world.frames)) map.set(frame.targetId, sessionId);
          return map;
        };
        const frameForOwner = (frameId) => {
          for (const frame of Object.values(world.frames)) {
            if (frame.targetId === frameId) return frame;
          }
          return null;
        };
        function agentAnswer(sessionId, expression) {
          const frame = world.frames[sessionId];
          if (!frame) return { result: { value: { ok: false, error: "FRAME_AGENT_UNAVAILABLE: no agent" } } };
          if (expression.includes('method: "install"')) {
            return { result: { value: { ok: true, result: { nonce: `nonce-${sessionId}` } } } };
          }
          const payload = JSON.parse(expression.slice(expression.indexOf("call(") + 5, expression.lastIndexOf(")")));
          calls.push({ agent: payload, sessionId });
          if (payload.nonce !== `nonce-${sessionId}`) {
            return { result: { value: { ok: false, error: "FRAME_AGENT_STALE: this frame agent belongs to an older control lease" } } };
          }
          if (payload.method === "snapshot") {
            return { result: { value: { ok: true, result: {
              agentGeneration: `agen-${sessionId}`,
              revision: 1,
              total: frame.elements.length,
              truncated: Boolean(frame.truncated),
              origin: "https://pay.example",
              elements: frame.elements,
            } } } };
          }
          const element = frame.elements.find((item) => item.key === payload.key);
          if (!element) return { result: { value: { ok: false, error: "STALE_REF: the element changed; observe the page again" } } };
          if (payload.method === "prepareClick") {
            return { result: { value: { ok: true, result: { ...element, key: element.key } } } };
          }
          if (payload.method === "commitClick") {
            return { result: { value: { ok: true, result: { validated: true, key: element.key, bounds: element.bounds } } } };
          }
          return { result: { value: { ok: false, error: "FRAME_AGENT_FAILED: unknown frame agent request" } } };
        }
        async function cdp(method, params, sessionId) {
          if (options.fail && options.fail(method, params, sessionId, calls)) {
            const code = options.failCode ?? "CDP_REJECTED";
            const error = new Error(`${code}: ${method}`);
            if (options.failCode) error.code = code;
            throw error;
          }
          switch (method) {
            case "Target.setAutoAttach":
            case "Target.detachFromTarget":
            case "DOM.enable":
            case "Page.enable":
            case "Input.dispatchMouseEvent":
              return {};
            case "DOM.getDocument":
              return { root: { nodeId: 1 } };
            case "Page.createIsolatedWorld":
              return { executionContextId: sessionId ? 200 + Object.keys(world.frames).indexOf(sessionId) : 11 };
            case "Page.getFrameTree": {
              if (!sessionId) return { frameTree: world.rootTree };
              if (world.routeStripsSession) return { frameTree: world.rootTree };
              return { frameTree: world.frames[sessionId].tree };
            }
            case "DOM.getFrameOwner": {
              const frame = frameForOwner(params.frameId);
              if (!frame?.owner) return {};
              return { backendNodeId: frame.owner.backendNodeId };
            }
            // Chrome answers with every quad; `border` models a real iframe
            // border, which is what getBoundingClientRect() below reports and
            // what the content quad deliberately does not include.
            case "DOM.getBoxModel": {
              const frame = Object.values(world.frames).find((item) => item.owner?.backendNodeId === params.backendNodeId);
              if (!frame?.owner) return {};
              const { x, y, width, height } = frame.owner;
              if (frame.owner.malformed) return { model: { content: [x, y, x + width], width, height } };
              const quad = (left, top, right, bottom) => [left, top, right, top, right, bottom, left, bottom];
              const content = quad(x, y, x + width, y + height);
              if (frame.owner.noBorderQuad) return { model: { content, width, height } };
              const edge = Number(frame.owner.border) || 0;
              return {
                model: {
                  content,
                  border: quad(x - edge, y - edge, x + width + edge, y + height + edge),
                  width,
                  height,
                },
              };
            }
            case "DOM.resolveNode":
              return { object: { objectId: `obj-${params.backendNodeId}` } };
            // getBoundingClientRect() reports the border box, so a bordered
            // iframe answers with a rect that is offset from its content quad
            // by exactly the border on every side.
            case "Runtime.callFunctionOn": {
              const frame = Object.values(world.frames).find((item) => `obj-${item.owner?.backendNodeId}` === params.objectId);
              const owner = frame?.owner ?? {};
              const edge = Number(owner.border) || 0;
              return { result: { value: {
                hit: world.ownerHit !== false,
                x: (owner.measuredX ?? owner.x) - edge,
                y: (owner.measuredY ?? owner.y) - edge,
                width: owner.width + 2 * edge,
                height: owner.height + 2 * edge,
              } } };
            }
            case "Runtime.evaluate":
              return agentAnswer(sessionId, params.expression);
            default:
              return {};
          }
        }
        const bridge = new Function("world", "calls", "cdp", "options", `
          const FRAME_MAX_ATTACHED = 16;
          const FRAME_MAX_DEPTH = 5;
          const FRAME_ELEMENT_CAP_TOTAL = 500;
          const FRAME_TOP_ELEMENT_CAP = 400;
          const FRAME_PER_FRAME_ELEMENT_CAP = 120;
          const FRAME_OFFSET_TOLERANCE_PX = 2;
          const FRAME_SKIP_REPORT_MAX = 32;
          const FRAME_AGENT_WORLD_NAME = "__lbb_frame_agent__";
          const FRAME_OBSERVE_BUDGET_MS = options.frameBudgetMs ?? 4_000;
          const attachedFrames = new Map();
          const frameParents = new Map();
          const frameSkips = [];
          const heldMouseInputs = new Map();
          let frameSupport = { supported: true, probed: true, reason: "" };
          let frameAgentSource = null;
          let frameTreeRevision = 0;
          let frameInvalidationReason = "";
          let frameSnapshot = null;
          let rootWorldContextId = 11;
          let controlLease = { tabId: 7, frameId: "ROOT", viewport: { width: 800, height: 600 } };
          globalThis.__LBB_DOM_CORE_SOURCE__ = () => "/* core */";
          globalThis.__LBB_FRAME_AGENT_SOURCE__ = () => "/* agent */";
          const authority = { tabId: 7, sessionId: "lease", epoch: 1, documentEpoch: 1 };
          const order = [];
          function assertLeaseAuthority(_authority, _context, boundary) { order.push(\`lease:\${boundary}\`); }
          function assertLeaseAuthorityAfterDispatch() {}
          function assertCommandActive() {}
          function sameDocumentUrl(left, right) { return String(left) === String(right); }
          function outcomeUnknownError(boundary, cause) {
            const error = new Error(\`ACTION_OUTCOME_UNKNOWN: control changed after \${boundary}; do not retry automatically\`);
            error.code = "ACTION_OUTCOME_UNKNOWN";
            error.cause = cause;
            return error;
          }
          function publicControlState() { return { active: true }; }
          function captureLeaseAuthority() { return authority; }
          async function debuggerCommand(tabId, method, params, _authority, _context, sessionId = null) {
            calls.push({ method, params, sessionId });
            if (method === "Input.dispatchMouseEvent") order.push(\`input:\${params.type}\`);
            if (options.afterCall) await options.afterCall(method, params, sessionId, api);
            return cdp(method, params, sessionId);
          }
          async function bestEffortDebuggerRelease(tabId, method, params, label) {
            calls.push({ method, params, label });
            return true;
          }
          async function contentRequest(tabId, payload) {
            calls.push({ content: payload.method });
            order.push(\`content:\${payload.method}\`);
            if (options.contentFails) throw new Error(options.contentFails);
            return { current: true };
          }
          async function verifyDocumentAuthority(tabId, _authority, _context, boundary) {
            order.push(\`document:\${boundary}\`);
            return { documentEpoch: 1 };
          }
          async function verifyDocumentAuthorityAfterDispatch(tabId, a, c, boundary) {
            if (options.documentFailsAfter === boundary) {
              throw outcomeUnknownError(boundary, new Error("the frame detached after dispatch"));
            }
            return verifyDocumentAuthority(tabId, a, c, boundary);
          }
          async function moveVirtualCursor(tabId, x, y) {
            order.push(\`pointer:\${x},\${y}\`);
            return { arrival: "arrived", moveSequence: 1, points: 4 };
          }
          async function persistHeldInputIntent(map, key, record) { map.set(key, record); order.push("held:persist"); }
          async function clearHeldInputIntent(map, key) { map.delete(key); order.push("held:clear"); }
          async function releaseHeldMouseInput(key, record) {
            if (heldMouseInputs.has(key)) order.push("held:release");
            heldMouseInputs.delete(key);
            return true;
          }
          const crypto = { randomUUID: () => "held-key" };
          ${frameFunctions}
          const api = {
            calls,
            order: () => [...order],
            attach: (sessionId, targetInfo, parentSessionId = null) =>
              recordAttachedTarget(7, sessionId, targetInfo, parentSessionId),
            event: (sessionId, method, params) => handleFrameSessionEvent({ tabId: 7, sessionId }, method, params),
            rootEvent: (method, params) => {
              if (method === "Target.detachedFromTarget" && dropFrameTargetTree(params.sessionId)) {
                markFrameTreeChanged("frame_target_detached");
              }
              if (method === "Page.frameResized") markFrameTreeChanged("frame_resized");
            },
            observe: (snapshot) => collectFrameObservations(7, snapshot, authority, null),
            merge: (top, results, skips, support) => mergeFrameObservations(top, results, skips, support),
            offset: (record, records) => accumulateFrameOffset(record, records),
            ownerOrigin: (boxModel) => ownerContentOrigin(boxModel),
            translate: (bounds, offset) => translateBounds(bounds, offset),
            inViewport: (translated, local, box, viewport) => frameElementInViewport(translated, local, box, viewport),
            parseRef: (ref) => parseFrameRef(ref),
            refuse: (ref, method) => assertTopLevelRef(ref, method),
            resolve: (ref, generation) => resolveFrameRecord(ref, generation),
            fresh: (generation) => assertFrameSnapshotFresh(generation),
            markChanged: (reason) => markFrameTreeChanged(reason),
            clear: () => clearFrameSessions(),
            records: () => [...attachedFrames.values()],
            recordFor: (ref) => [...attachedFrames.values()].find((record) => record.ref === ref),
            support: () => ({ ...frameSupport }),
            setSupport: (support, contextId = null) => {
              frameSupport = { ...support };
              rootWorldContextId = contextId;
            },
            revision: () => frameTreeRevision,
            invalidation: () => frameInvalidationReason,
            snapshot: () => frameSnapshot,
            publish: (generation) => {
              const frames = new Map();
              for (const record of attachedFrames.values()) {
                if (record.ref) frames.set(record.ref, record);
              }
              frameSnapshot = { generation, frameTreeRevision, frames };
              return frameSnapshot;
            },
            lookups: () => [...lookups],
            publishWatched: (generation) => {
              const real = new Map();
              for (const record of attachedFrames.values()) {
                if (record.ref) real.set(record.ref, record);
              }
              frameSnapshot = {
                generation,
                frameTreeRevision,
                frames: { get(key) { lookups.push(key); return real.get(key); } },
              };
              return frameSnapshot;
            },
            click: async (ref, generation) => {
              const target = await prepareFrameTarget(7, ref, generation, authority, null, "frame click preparation");
              return trustedClick(
                7, target.description, ref, generation, { button: "left", clickCount: 1, modifiers: 0 },
                null, authority,
                frameClickHooks(7, target.record, target.key, target.description, authority, null),
              );
            },
          };
          const lookups = [];
          return api;
        `)(world, calls, cdp, options);
        return bridge;
      }
"##;

/// Shared Node harness that runs the real shared DOM core and the real
/// frame agent against a stubbed frame document.
const FRAME_AGENT_HARNESS_PRELUDE: &str = r##"      import fs from "node:fs";
      function extractFunction(source, name) {
        const marker = `function ${name}(`;
        let start = source.indexOf(marker);
        if (start < 0) throw new Error(`missing ${name}`);
        if (source.slice(start - 6, start) === "async ") start -= 6;
        const signatureEnd = source.indexOf(") {", source.indexOf(marker));
        const brace = signatureEnd + 2;
        let depth = 0, quote = "", escaped = false;
        for (let index = brace; index < source.length; index += 1) {
          const character = source[index];
          if (quote) {
            if (escaped) escaped = false;
            else if (character === "\\") escaped = true;
            else if (character === quote) quote = "";
          } else if (["\"", "'", "`"].includes(character)) quote = character;
          else if (character === "{") depth += 1;
          else if (character === "}" && --depth === 0) return source.slice(start, index + 1);
        }
        throw new Error(`unterminated ${name}`);
      }
      const coreSource = fs.readFileSync("extension/dom-core.js", "utf8");
      const agentSource = fs.readFileSync("extension/frame-agent.js", "utf8");
      const domCore = [
        extractFunction(coreSource, "createRevisionTracker"),
        extractFunction(coreSource, "createDomCore"),
      ].join("\n");
      const frameAgent = extractFunction(agentSource, "createFrameAgent");

      // A deliberately minimal frame: every rectangle below is frame-local,
      // so any top-level coordinate leaking into the agent would be visible.
      function makeAgent(specs) {
        const hitTests = [];
        const mutationCallbacks = [];
        const listeners = [];
        return new Function("specs", "hitTests", "mutationCallbacks", "listeners", `
          class Element {}
          class HTMLInputElement extends Element {}
          class HTMLTextAreaElement extends Element {}
          class HTMLAnchorElement extends Element {}
          class ShadowRoot {}
          class StubElement extends Element {
            constructor(spec) {
              super();
              this.tagName = spec.tagName ?? "BUTTON";
              this.attributes = { ...(spec.attributes ?? {}) };
              this.rect = { ...spec.rect };
              this.innerText = spec.text ?? "";
              this.textContent = spec.text ?? "";
              this.alt = ""; this.title = ""; this.placeholder = ""; this.id = "";
              this.isConnected = true;
              this.shadowRoot = null;
            }
            getAttribute(name) { return this.attributes[name] ?? null; }
            getBoundingClientRect() {
              const rect = this.rect;
              return {
                x: rect.x, y: rect.y, width: rect.width, height: rect.height,
                top: rect.y, left: rect.x, right: rect.x + rect.width, bottom: rect.y + rect.height,
              };
            }
            getRootNode() { return document; }
          }
          const elements = specs.map((spec) => new StubElement(spec));
          const innerWidth = 400;
          const innerHeight = 300;
          const scrollX = 0;
          const scrollY = 0;
          const devicePixelRatio = 1;
          const location = { origin: "https://pay.example", pathname: "/checkout" };
          const document = {
            documentElement: { scrollHeight: 300 },
            title: "Frame",
            querySelectorAll(selector) { return selector === "*" ? [] : elements.filter((element) => !element.hidden); },
            getElementById() { return null; },
            elementFromPoint(x, y) {
              hitTests.push({ x, y });
              return elements.find((element) => {
                const rect = element.getBoundingClientRect();
                return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
              }) ?? null;
            },
          };
          function getComputedStyle(element) {
            return element.hiddenStyle
              ? { display: "none", visibility: "hidden", opacity: "0" }
              : { display: "block", visibility: "visible", opacity: "1" };
          }
          class MutationObserver {
            constructor(callback) { mutationCallbacks.push(callback); }
            observe() {}
          }
          function addEventListener(name, handler) { listeners.push({ name, handler }); }
          ${domCore}
          ${frameAgent}
          const agent = createFrameAgent(createDomCore({}));
          return {
            call: (request) => agent.call(request),
            elements,
            hitTests: () => [...hitTests],
            mutate: (records) => {
              for (const callback of mutationCallbacks) callback(records ?? [{ target: null, addedNodes: [], removedNodes: [] }]);
            },
            scroll: () => {
              for (const listener of listeners) {
                if (listener.name === "scroll") listener.handler();
              }
            },
          };
        `)(specs, hitTests, mutationCallbacks, listeners);
      }

      // The real installable body the service worker evaluates into a frame
      // world, run against a stubbed document. `globalThis` is shadowed, so
      // the world it publishes into is this harness and not the Node global.
      function makeInstaller() {
        const installOnce = extractFunction(agentSource, "installFrameAgentOnce");
        const mutationCallbacks = [];
        const listeners = [];
        return new Function("mutationCallbacks", "listeners", `
          const globalThis = {};
          const innerWidth = 400;
          const innerHeight = 300;
          const scrollX = 0;
          const scrollY = 0;
          const devicePixelRatio = 1;
          const location = { origin: "https://pay.example", pathname: "/checkout" };
          const document = {
            documentElement: { scrollHeight: 300 },
            title: "Frame",
            querySelectorAll() { return []; },
            getElementById() { return null; },
            elementFromPoint() { return null; },
          };
          class MutationObserver {
            constructor(callback) { mutationCallbacks.push(callback); }
            observe() {}
          }
          function addEventListener(name, handler) { listeners.push({ name, handler }); }
          ${domCore}
          globalThis.__LBB_DOM_CORE__ = createDomCore;
          ${frameAgent}
          ${installOnce}
          return {
            install: () => installFrameAgentOnce(),
            agent: () => globalThis.__LBB_FRAME_AGENT__,
            observers: () => mutationCallbacks.length,
            listeners: () => listeners.map((listener) => listener.name),
          };
        `)(mutationCallbacks, listeners);
      }
"##;

fn run_frame_harness(label: &str, script: &str) {
    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run {label}: {error}"),
    };
    assert!(
        output.status.success(),
        "Node {label} failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn frame_session_routing_probe_fails_closed_when_session_is_stripped() {
    run_frame_harness(
        "frame routing probe harness",
        &format!(
            "{FRAME_HARNESS_PRELUDE}{}",
            r##"      // Chrome >= 125: the child session answers with its OWN frame tree.
      const routed = makeFrameBridge(defaultWorld());
      await routed.attach("child-1", { type: "iframe", targetId: "F1", url: "https://pay.example/" });
      const merged = await routed.observe({
        generation: "g1",
        elements: [{ ref: "g1.e1", role: "link", name: "Top" }],
        frameOwners: [{ origin: "https://pay.example" }],
      });
      if (merged.frameSummary.supported !== true || merged.frames.length !== 1) {
        throw new Error(`routed session did not merge: ${JSON.stringify(merged.frameSummary)}`);
      }
      if (merged.elements.length !== 2 || merged.elements[1].ref !== "g1.f1.e1") {
        throw new Error(`frame element ref is wrong: ${JSON.stringify(merged.elements[1])}`);
      }
      if (merged.elements[1].crossOrigin !== true || merged.elements[1].frameRef !== "f1") {
        throw new Error("merged element lost its frame provenance");
      }
      if ("key" in merged.elements[1] || "proof" in merged.elements[1]) {
        throw new Error("a merged element leaked its frame-local key or proof");
      }
      if (merged.elements[0].frameRef !== undefined || merged.elements[0].crossOrigin !== undefined) {
        throw new Error("a top-document element gained frame provenance");
      }

      // Counterfactual faulty route: sessionId is stripped and the child call
      // answers with the ROOT frame tree. Nothing may be merged from it.
      const stripped = makeFrameBridge(defaultWorld({ routeStripsSession: true }));
      await stripped.attach("child-1", { type: "iframe", targetId: "F1", url: "https://pay.example/" });
      const fallback = await stripped.observe({
        generation: "g1",
        elements: [{ ref: "g1.e1", role: "link", name: "Top" }],
        frameOwners: [{ origin: "https://pay.example" }],
      });
      if (fallback.frameSummary.supported !== false
        || fallback.frameSummary.reason !== "session_routing_unverified") {
        throw new Error(`a stripped child session was not refused: ${JSON.stringify(fallback.frameSummary)}`);
      }
      if (fallback.frames.length !== 0 || stripped.records().length !== 0) {
        throw new Error("an unverified session left records behind");
      }
      if (fallback.elements.length !== 1 || fallback.elements[0].ref !== "g1.e1") {
        throw new Error("the fallback observation was not exactly the top document");
      }
      const childCalls = stripped.calls.filter((call) => call.sessionId === "child-1").map((call) => call.method);
      if (childCalls.join(",") !== "Page.enable,Page.getFrameTree") {
        throw new Error(`an unverified session received more commands: ${childCalls.join(",")}`);
      }
      if (fallback.frameSummary.skipped.some((skip) => skip.reason !== "same_process_frame")) {
        throw new Error("unexpected skip reasons after a failed routing probe");
      }
      const callsAfterRefusal = stripped.calls.length;
      const refusedAgain = await stripped.observe({
        generation: "g2",
        elements: [{ ref: "g2.e1", role: "link", name: "Top again" }],
        frameOwners: [{ origin: "https://pay.example" }],
      });
      if (refusedAgain.frameSummary.supported !== false
        || refusedAgain.frameSummary.reason !== "session_routing_unverified"
        || refusedAgain.frames.length !== 0) {
        throw new Error(`a routing refusal was not sticky for the lease: ${JSON.stringify(refusedAgain.frameSummary)}`);
      }
      if (stripped.calls.length !== callsAfterRefusal) {
        throw new Error(`a completed negative routing probe was retried: ${JSON.stringify(stripped.calls.slice(callsAfterRefusal))}`);
      }

      const transient = makeFrameBridge(defaultWorld());
      transient.setSupport({ supported: false, probed: true, reason: "auto_attach_unavailable" });
      const retried = await transient.observe({ generation: "g1", elements: [], frameOwners: [] });
      if (retried.frameSummary.supported !== true
        || !transient.calls.some((call) => call.method === "Target.setAutoAttach" && call.sessionId === null)) {
        throw new Error(`a transient root auto-attach failure was not retried: ${JSON.stringify(retried.frameSummary)}`);
      }

      // Targets refused at attach time are detached immediately and their
      // reason survives into the next observation.
      const filtered = makeFrameBridge(defaultWorld());
      await filtered.attach("worker-1", { type: "service_worker", targetId: "W1", url: "https://pay.example/sw.js" });
      await filtered.attach("blank-1", { type: "iframe", targetId: "B1", url: "about:blank" });
      await filtered.attach("child-1", { type: "iframe", targetId: "F1", url: "https://pay.example/" });
      if (filtered.records().length !== 1) {
        throw new Error(`a non-iframe or blank target stayed attached: ${filtered.records().length}`);
      }
      const detaches = filtered.calls.filter((call) => call.method === "Target.detachFromTarget");
      if (detaches.length !== 2) {
        throw new Error(`refused targets were not detached immediately: ${detaches.length}`);
      }
      const withSkips = await filtered.observe({ generation: "g1", elements: [], frameOwners: [] });
      if (!withSkips.frameSummary.skipped.some((skip) => skip.reason === "blank_document")) {
        throw new Error(`an attach-time skip was lost: ${JSON.stringify(withSkips.frameSummary.skipped)}`);
      }
      const again = await filtered.observe({ generation: "g1", elements: [], frameOwners: [] });
      if (again.frameSummary.skipped.some((skip) => skip.reason === "blank_document")) {
        throw new Error("an attach-time skip was reported twice");
      }
"##
        ),
    );
}

#[test]
fn frame_auto_attach_recurses_through_verified_child_sessions() {
    run_frame_harness(
        "recursive frame auto-attach harness",
        &format!(
            "{FRAME_HARNESS_PRELUDE}{}",
            r##"      const world = {
        routeStripsSession: false,
        ownerHit: true,
        rootTree: {
          frame: { id: "ROOT", loaderId: "L0", url: "https://top.example/" },
          childFrames: [{ frame: { id: "F1", parentId: "ROOT", url: "https://pay.example/" } }],
        },
        frames: {
          "child-1": {
            targetId: "F1",
            tree: {
              frame: { id: "F1", parentId: "ROOT", loaderId: "L1", url: "https://pay.example/" },
              childFrames: [{ frame: { id: "F2", parentId: "F1", url: "https://deep.example/" } }],
            },
            owner: { backendNodeId: 501, x: 120, y: 64, width: 380, height: 220 },
            elements: [{
              key: "e1", role: "button", name: "Outer", inViewport: true,
              bounds: { x: 3, y: 3, width: 30, height: 15 },
            }],
          },
          "child-2": {
            targetId: "F2",
            tree: { frame: { id: "F2", parentId: "F1", loaderId: "L2", url: "https://deep.example/" } },
            owner: { backendNodeId: 502, x: 10, y: 10, width: 200, height: 120 },
            elements: [{
              key: "e1", role: "button", name: "Inner", inViewport: true,
              bounds: { x: 5, y: 5, width: 40, height: 20 },
            }],
          },
        },
      };
      let emittedGrandchild = false;
      const bridge = makeFrameBridge(world, {
        afterCall: async (method, params, sessionId, api) => {
          if (method === "Target.setAutoAttach" && sessionId === "child-1" && !emittedGrandchild) {
            emittedGrandchild = true;
            api.event("child-1", "Target.attachedToTarget", {
              sessionId: "child-2",
              targetInfo: { type: "iframe", targetId: "F2", url: "https://deep.example/" },
            });
          }
        },
      });
      await bridge.attach("child-1", {
        type: "iframe", targetId: "F1", url: "https://pay.example/",
      });
      const observed = await bridge.observe({ generation: "g1", elements: [], frameOwners: [] });
      if (observed.frames.length !== 2 || observed.frames[1].depth !== 2) {
        throw new Error(`recursive discovery did not merge depth two: ${JSON.stringify(observed.frameSummary)}`);
      }
      const inner = observed.elements.find((element) => element.name === "Inner");
      if (!inner || JSON.stringify(inner.bounds) !== JSON.stringify({ x: 135, y: 79, width: 40, height: 20 })) {
        throw new Error(`recursive discovery published wrong bounds: ${JSON.stringify(inner)}`);
      }
      const armed = bridge.calls.filter((call) => call.method === "Target.setAutoAttach");
      if (armed.map((call) => call.sessionId).join(",") !== "child-1,child-2") {
        throw new Error(`child sessions were not recursively armed: ${JSON.stringify(armed)}`);
      }
      for (const call of armed) {
        const expected = { autoAttach: true, waitForDebuggerOnStart: false, flatten: true };
        if (JSON.stringify(call.params) !== JSON.stringify(expected)) {
          throw new Error(`recursive auto-attach params drifted: ${JSON.stringify(call.params)}`);
        }
      }
      await bridge.observe({ generation: "g2", elements: [], frameOwners: [] });
      const armedAgain = bridge.calls.filter((call) => call.method === "Target.setAutoAttach");
      if (armedAgain.length !== armed.length) {
        throw new Error(`an already armed child was re-armed: ${JSON.stringify(armedAgain)}`);
      }

      bridge.publish("g1");
      world.frames["child-2"].tree.childFrames = [
        { frame: { id: "F3", parentId: "F2", url: "https://later.example/" } },
      ];
      world.frames["child-3"] = {
        targetId: "F3",
        tree: { frame: { id: "F3", parentId: "F2", loaderId: "L3", url: "https://later.example/" } },
        owner: { backendNodeId: 503, x: 8, y: 8, width: 100, height: 60 },
        elements: [{
          key: "e1", role: "button", name: "Later", inViewport: true,
          bounds: { x: 2, y: 2, width: 20, height: 10 },
        }],
      };
      const beforeAttach = bridge.revision();
      bridge.event("child-2", "Target.attachedToTarget", {
        sessionId: "child-3",
        targetInfo: { type: "iframe", targetId: "F3", url: "https://later.example/" },
      });
      if (bridge.revision() !== beforeAttach + 1 || !bridge.records().some((record) => record.sessionId === "child-3")) {
        throw new Error("a late nested attachment was not admitted and invalidated");
      }
      let stale = null;
      try { bridge.fresh("g1"); } catch (error) { stale = error; }
      if (!/frame_attached/.test(stale?.message ?? "")) {
        throw new Error(`a late nested attachment left the snapshot actionable: ${stale?.message}`);
      }
      const later = await bridge.observe({ generation: "g3", elements: [], frameOwners: [] });
      if (later.frames.length !== 3 || later.frames[2].depth !== 3
        || !later.elements.some((element) => element.name === "Later")) {
        throw new Error(`a late nested attachment was not merged on the next observation: ${JSON.stringify(later.frameSummary)}`);
      }
      const lateArm = bridge.calls.filter((call) => call.method === "Target.setAutoAttach" && call.sessionId === "child-3");
      if (lateArm.length !== 1) throw new Error(`a late child was not armed exactly once: ${lateArm.length}`);
      bridge.event("child-3", "Target.attachedToTarget", {
        sessionId: "child-4",
        targetInfo: { type: "iframe", targetId: "F4", url: "https://deeper.example/" },
      });
      if (!bridge.records().some((record) => record.sessionId === "child-4")) {
        throw new Error("a deeper nested attachment was not admitted");
      }
      const beforeDetach = bridge.revision();
      bridge.event("child-2", "Target.detachedFromTarget", { sessionId: "child-3" });
      if (bridge.revision() !== beforeDetach + 1
        || bridge.records().some((record) => ["child-3", "child-4"].includes(record.sessionId))) {
        throw new Error("a nested detach event left its subtree actionable");
      }

      const unknown = makeFrameBridge(defaultWorld());
      unknown.event("missing-parent", "Target.attachedToTarget", {
        sessionId: "orphan",
        targetInfo: { type: "iframe", targetId: "ORPHAN", url: "https://orphan.example/" },
      });
      if (unknown.records().length !== 0
        || !unknown.calls.some((call) => call.method === "Target.detachFromTarget" && call.params.sessionId === "orphan")) {
        throw new Error("a nested target from an unknown parent was not detached");
      }

      const chainFrames = {};
      for (let depth = 1; depth <= 5; depth += 1) {
        const frameId = `D${depth}`;
        const parentId = depth === 1 ? "ROOT" : `D${depth - 1}`;
        chainFrames[`depth-${depth}`] = {
          targetId: frameId,
          tree: {
            frame: { id: frameId, parentId, loaderId: `LD${depth}`, url: `https://d${depth}.example/` },
            ...(depth < 5 ? { childFrames: [{ frame: {
              id: `D${depth + 1}`, parentId: frameId, url: `https://d${depth + 1}.example/`,
            } }] } : {}),
          },
          owner: { backendNodeId: 600 + depth, x: 1, y: 1, width: 100, height: 80 },
          elements: [],
        };
      }
      const chainWorld = {
        routeStripsSession: false,
        ownerHit: true,
        rootTree: {
          frame: { id: "ROOT", loaderId: "L0", url: "https://top.example/" },
          childFrames: [{ frame: { id: "D1", parentId: "ROOT", url: "https://d1.example/" } }],
        },
        frames: chainFrames,
      };
      const emittedDepths = new Set();
      const bounded = makeFrameBridge(chainWorld, {
        afterCall: async (method, params, sessionId, api) => {
          const depth = Number(String(sessionId ?? "").replace("depth-", ""));
          if (method !== "Target.setAutoAttach" || !Number.isInteger(depth) || emittedDepths.has(depth)) return;
          emittedDepths.add(depth);
          api.event(sessionId, "Target.attachedToTarget", {
            sessionId: `depth-${depth + 1}`,
            targetInfo: {
              type: "iframe", targetId: `D${depth + 1}`, url: `https://d${depth + 1}.example/`,
            },
          });
        },
      });
      await bounded.attach("depth-1", {
        type: "iframe", targetId: "D1", url: "https://d1.example/",
      });
      const depthBound = await bounded.observe({ generation: "g1", elements: [], frameOwners: [] });
      const depthArms = bounded.calls.filter((call) => call.method === "Target.setAutoAttach");
      if (bounded.records().length !== 5 || depthArms.length !== 5
        || !bounded.calls.some((call) => call.method === "Target.detachFromTarget" && call.params.sessionId === "depth-6")
        || !depthBound.frameSummary.skipped.some((skip) => skip.reason === "depth_exceeded")) {
        throw new Error(`recursive attachment did not report and detach depth six: ${JSON.stringify(depthBound.frameSummary)}`);
      }

      for (const failCode of [null, "FRAME_OBSERVE_TIMEOUT"]) {
        const failed = makeFrameBridge(defaultWorld(), {
          fail: (method, params, sessionId) => method === "Target.setAutoAttach" && sessionId === "child-1",
          ...(failCode ? { failCode } : {}),
        });
        await failed.attach("child-1", {
          type: "iframe", targetId: "F1", url: "https://pay.example/",
        });
        const partial = await failed.observe({ generation: "g1", elements: [], frameOwners: [] });
        const expectedReason = failCode ? "frame_timeout" : "session_probe_failed";
        if (partial.frameSummary.supported !== true || partial.frames.length !== 1
          || !partial.frameSummary.skipped.some((skip) => skip.reason === expectedReason)) {
          throw new Error(`a recursive arm failure was not branch-local (${expectedReason}): ${JSON.stringify(partial.frameSummary)}`);
        }
      }

      const canceled = makeFrameBridge(defaultWorld(), {
        fail: (method, params, sessionId) => method === "Target.setAutoAttach" && sessionId === "child-1",
        failCode: "CONTROL_CANCELED",
      });
      await canceled.attach("child-1", {
        type: "iframe", targetId: "F1", url: "https://pay.example/",
      });
      let cancellation = null;
      try {
        await canceled.observe({ generation: "g1", elements: [], frameOwners: [] });
      } catch (error) { cancellation = error; }
      if (cancellation?.code !== "CONTROL_CANCELED") {
        throw new Error(`a lease-fatal recursive arm failure was swallowed: ${cancellation?.message}`);
      }
"##
        ),
    );
}

/// Frame latency and frame refusals are reported, never charged to the lease
/// and never charged twice to the next observation.
#[test]
fn frame_latency_and_skips_are_reported_once_and_never_revoke_the_lease() {
    run_frame_harness(
        "frame latency harness",
        &format!(
            "{FRAME_HARNESS_PRELUDE}{}",
            r##"      const countReasons = (merged, reason) =>
        merged.frameSummary.skipped.filter((skip) => skip.reason === reason).length;

      // A skip recorded DURING an observation belongs to that observation
      // only. Leaving it in the pending list would report it again next turn,
      // and sixteen frames of duplicates saturate the 32-entry skip report.
      const unresolved = defaultWorld();
      delete unresolved.frames["child-1"].owner;
      const repeating = makeFrameBridge(unresolved);
      await repeating.attach("child-1", { type: "iframe", targetId: "F1", url: "https://pay.example/" });
      const first = await repeating.observe({ generation: "g1", elements: [], frameOwners: [] });
      if (countReasons(first, "owner_unresolved") !== 1) {
        throw new Error(`an in-observation skip was not reported once: ${JSON.stringify(first.frameSummary.skipped)}`);
      }
      const second = await repeating.observe({ generation: "g2", elements: [], frameOwners: [] });
      if (countReasons(second, "owner_unresolved") !== 1) {
        throw new Error(`an in-observation skip was reported twice: ${JSON.stringify(second.frameSummary.skipped)}`);
      }

      // A frame that stops answering is one frame_timeout skip on a healthy
      // observation: the read-only frame pass never revokes the lease over
      // third-party latency.
      const slow = makeFrameBridge(defaultWorld(), {
        fail: (method) => method === "Runtime.evaluate",
        failCode: "FRAME_OBSERVE_TIMEOUT",
      });
      await slow.attach("child-1", { type: "iframe", targetId: "F1", url: "https://pay.example/" });
      const timedOut = await slow.observe({ generation: "g1", elements: [], frameOwners: [] });
      if (timedOut.frameSummary.supported !== true || timedOut.frames.length !== 0) {
        throw new Error(`a slow frame broke the observation: ${JSON.stringify(timedOut.frameSummary)}`);
      }
      if (countReasons(timedOut, "frame_timeout") !== 1) {
        throw new Error(`a frame timeout was not reported as itself: ${JSON.stringify(timedOut.frameSummary.skipped)}`);
      }

      // Any other CDP refusal keeps its own diagnosis.
      const refusing = makeFrameBridge(defaultWorld(), { fail: (method) => method === "Runtime.evaluate" });
      await refusing.attach("child-1", { type: "iframe", targetId: "F1", url: "https://pay.example/" });
      const refused = await refusing.observe({ generation: "g1", elements: [], frameOwners: [] });
      if (countReasons(refused, "agent_install_failed") !== 1) {
        throw new Error(`a refused frame agent lost its reason: ${JSON.stringify(refused.frameSummary.skipped)}`);
      }

      // The pass as a whole is bounded, not just each command inside it: a
      // spent budget reports the remaining frames instead of waiting on them.
      const spent = makeFrameBridge(defaultWorld(), { frameBudgetMs: 0 });
      await spent.attach("child-1", { type: "iframe", targetId: "F1", url: "https://pay.example/" });
      const bounded = await spent.observe({ generation: "g1", elements: [], frameOwners: [] });
      if (bounded.frames.length !== 0 || countReasons(bounded, "budget_time") !== 1) {
        throw new Error(`a spent frame budget was not reported: ${JSON.stringify(bounded.frameSummary)}`);
      }
      if (spent.calls.some((call) => call.method === "Target.setAutoAttach" && call.sessionId)) {
        throw new Error("a spent frame budget still armed a descendant session");
      }
"##
        ),
    );
}

#[test]
fn frame_offsets_accumulate_through_nested_frames() {
    run_frame_harness(
        "frame offset harness",
        &format!(
            "{FRAME_HARNESS_PRELUDE}{}",
            r##"      const bridge = makeFrameBridge();
      const records = new Map([
        ["s1", { sessionId: "s1", parentSessionId: null, ownerOrigin: { x: 120, y: 64, width: 380, height: 220 } }],
        ["s2", { sessionId: "s2", parentSessionId: "s1", ownerOrigin: { x: 10, y: 10, width: 200, height: 120 } }],
      ]);
      const offset = bridge.offset(records.get("s2"), records);
      if (offset.x !== 130 || offset.y !== 74) {
        throw new Error(`nested offsets did not accumulate: ${JSON.stringify(offset)}`);
      }
      const translated = bridge.translate({ x: 5, y: 5, width: 40, height: 20 }, offset);
      if (JSON.stringify(translated) !== JSON.stringify({ x: 135, y: 79, width: 40, height: 20 })) {
        throw new Error(`translation is wrong: ${JSON.stringify(translated)}`);
      }

      const deep = new Map();
      let parent = null;
      for (let index = 1; index <= 7; index += 1) {
        const sessionId = `d${index}`;
        deep.set(sessionId, { sessionId, parentSessionId: parent, ownerOrigin: { x: 1, y: 1, width: 10, height: 10 } });
        parent = sessionId;
      }
      let depthError = null;
      try { bridge.offset(deep.get("d7"), deep); } catch (error) { depthError = error; }
      if (depthError?.code !== "STALE_FRAME_TREE" || !/nesting exceeded/.test(depthError.message)) {
        throw new Error(`over-deep nesting was not refused: ${depthError && depthError.message}`);
      }
      const shallow = new Map([...deep].slice(0, 5));
      shallow.get("d1").parentSessionId = null;
      if (bridge.offset(shallow.get("d5"), shallow).x !== 5) {
        throw new Error("a legal five-deep chain was refused");
      }

      let unresolved = null;
      try {
        bridge.offset({ sessionId: "x", parentSessionId: null, ownerOrigin: null }, new Map());
      } catch (error) { unresolved = error; }
      if (!/owner element is unresolved/.test(unresolved?.message ?? "")) {
        throw new Error("an unresolved owner did not fail closed");
      }

      if (bridge.ownerOrigin({ model: { content: [1, 2, 3], width: 4, height: 5 } }) !== null) {
        throw new Error("a malformed content quad was accepted");
      }
      if (bridge.ownerOrigin(undefined) !== null) throw new Error("a missing box model was accepted");
      const origin = bridge.ownerOrigin({
        model: { content: [500, 284, 120, 284, 120, 64, 500, 64], width: 380, height: 220 },
      });
      if (origin.x !== 120 || origin.y !== 64) {
        throw new Error(`quad corner selection is wrong: ${JSON.stringify(origin)}`);
      }

      // An element that is inside its own frame's viewport but scrolled past
      // the frame's visible box on the page is not reachable.
      const frameBox = { x: 130, y: 74, width: 200, height: 120 };
      const viewport = { width: 800, height: 600 };
      if (bridge.inViewport({ x: 135, y: 79, width: 40, height: 20 }, true, frameBox, viewport) !== true) {
        throw new Error("a visible frame element was reported out of viewport");
      }
      if (bridge.inViewport({ x: 135, y: 400, width: 40, height: 20 }, true, frameBox, viewport) !== false) {
        throw new Error("an element below the frame box was reported in viewport");
      }
      if (bridge.inViewport({ x: 135, y: 79, width: 40, height: 20 }, false, frameBox, viewport) !== false) {
        throw new Error("a frame-local out-of-viewport element was reported in viewport");
      }
      if (bridge.inViewport({ x: 900, y: 79, width: 40, height: 20 }, true, { x: 900, y: 74, width: 200, height: 120 }, viewport) !== false) {
        throw new Error("an element outside the top viewport was reported in viewport");
      }

      // End to end through two out-of-process boundaries: the published
      // bounds are frame-local plus the accumulated offset, and the frames are
      // numbered depth-first in owner-element document order.
      const nested = makeFrameBridge({
        routeStripsSession: false,
        ownerHit: true,
        rootTree: {
          frame: { id: "ROOT", loaderId: "L0", url: "https://top.example/" },
          childFrames: [{ frame: { id: "F1", parentId: "ROOT", url: "https://pay.example/" } }],
        },
        frames: {
          "child-1": {
            targetId: "F1",
            tree: {
              frame: { id: "F1", parentId: "ROOT", loaderId: "L1", url: "https://pay.example/" },
              childFrames: [{ frame: { id: "F2", parentId: "F1", url: "https://deep.example/" } }],
            },
            owner: { backendNodeId: 501, x: 120, y: 64, width: 380, height: 220 },
            elements: [{
              key: "e1", role: "button", name: "Outer", inViewport: true,
              bounds: { x: 3, y: 3, width: 30, height: 15 },
            }],
          },
          "child-2": {
            targetId: "F2",
            tree: { frame: { id: "F2", parentId: "F1", loaderId: "L2", url: "https://deep.example/" } },
            owner: { backendNodeId: 502, x: 10, y: 10, width: 200, height: 120 },
            elements: [{
              key: "e1", role: "button", name: "Inner", inViewport: true,
              bounds: { x: 5, y: 5, width: 40, height: 20 },
            }],
          },
        },
      });
      await nested.attach("child-2", { type: "iframe", targetId: "F2", url: "https://deep.example/" });
      await nested.attach("child-1", { type: "iframe", targetId: "F1", url: "https://pay.example/" });
      const observed = await nested.observe({ generation: "g1", elements: [], frameOwners: [] });
      if (observed.frames.length !== 2) {
        throw new Error(`nested frames were not both merged: ${JSON.stringify(observed.frameSummary)}`);
      }
      if (observed.frames[0].frameId !== "F1" || observed.frames[1].frameId !== "F2") {
        throw new Error("frames were not numbered depth-first in owner document order");
      }
      if (observed.frames[1].depth !== 2) throw new Error("nested frame depth is wrong");
      if (JSON.stringify(observed.frames[1].offset) !== JSON.stringify({ x: 130, y: 74 })) {
        throw new Error(`nested frame offset is wrong: ${JSON.stringify(observed.frames[1].offset)}`);
      }
      const inner = observed.elements.find((element) => element.ref === "g1.f2.e1");
      if (!inner) throw new Error("the deeply nested element was not published");
      if (JSON.stringify(inner.bounds) !== JSON.stringify({ x: 135, y: 79, width: 40, height: 20 })) {
        throw new Error(`nested element bounds are wrong: ${JSON.stringify(inner.bounds)}`);
      }
      if (inner.frameUrlOrigin !== "https://deep.example") {
        throw new Error("the nested element lost its own origin");
      }
      const outer = observed.elements.find((element) => element.ref === "g1.f1.e1");
      if (JSON.stringify(outer.bounds) !== JSON.stringify({ x: 123, y: 67, width: 30, height: 15 })) {
        throw new Error(`outer element bounds are wrong: ${JSON.stringify(outer.bounds)}`);
      }
"##
        ),
    );
}

#[test]
fn frame_observation_merges_within_the_element_budget() {
    run_frame_harness(
        "frame merge harness",
        &format!(
            "{FRAME_HARNESS_PRELUDE}{}",
            r##"      const bridge = makeFrameBridge();
      const elementsFor = (count, prefix) => Array.from({ length: count }, (_, index) => ({
        key: `e${index + 1}`, ref: `${prefix}${index + 1}`, role: "button", name: `${prefix}${index + 1}`,
      }));
      const frameResult = (index, count) => ({
        frameId: `F${index}`,
        urlOrigin: `https://f${index}.example`,
        depth: 1,
        offset: { x: 10 * index, y: 10 * index },
        size: { width: 100, height: 100 },
        truncated: false,
        elements: elementsFor(count, `f${index}-`),
      });

      const crowded = bridge.merge(
        { generation: "g1", elements: elementsFor(450, "t"), frameOwners: [] },
        [frameResult(1, 200), frameResult(2, 200), frameResult(3, 200)],
        [],
        { supported: true, attached: 3 },
      );
      if (crowded.elements.length !== 500) {
        throw new Error(`total element budget was not 500: ${crowded.elements.length}`);
      }
      const topKept = crowded.elements.filter((element) => !element.frameRef).length;
      if (topKept !== 400) throw new Error(`top elements were not reserved at 400: ${topKept}`);
      if (crowded.frames.length !== 1 || crowded.frames[0].ref !== "f1") {
        throw new Error(`only the first frame should fit: ${JSON.stringify(crowded.frames)}`);
      }
      if (crowded.frames[0].elementCount !== 100 || crowded.frames[0].truncated !== true) {
        throw new Error(`frame truncation was not reported: ${JSON.stringify(crowded.frames[0])}`);
      }
      const budgetSkips = crowded.frameSummary.skipped.filter((skip) => skip.reason === "budget_elements");
      if (budgetSkips.length !== 2) {
        throw new Error(`budget_elements was not reported twice: ${JSON.stringify(crowded.frameSummary.skipped)}`);
      }
      if (crowded.frameSummary.elementsDropped !== 50 + 100 + 200 + 200) {
        throw new Error(`elementsDropped is wrong: ${crowded.frameSummary.elementsDropped}`);
      }
      const refs = crowded.elements.map((element) => element.ref);
      if (new Set(refs).size !== refs.length) throw new Error("merged refs contain a duplicate");
      if (crowded.elements[400].ref !== "g1.f1.e1") {
        throw new Error(`frame refs are not <generation>.<frame>.<element>: ${crowded.elements[400].ref}`);
      }

      // Frame count budget.
      const many = bridge.merge(
        { generation: "g1", elements: [], frameOwners: [] },
        Array.from({ length: 20 }, (_, index) => frameResult(index + 1, 2)),
        [],
        { supported: true, attached: 20 },
      );
      if (many.frames.length !== 16) throw new Error(`frame budget is not 16: ${many.frames.length}`);
      if (many.frames[15].ref !== "f16") throw new Error("frame refs are not contiguous f1..f16");
      if (many.frameSummary.skipped.filter((skip) => skip.reason === "budget_frames").length !== 4) {
        throw new Error("budget_frames was not reported for the four dropped frames");
      }

      // A frameless page keeps the full 500-element budget and publishes no
      // frame provenance at all.
      const plain = bridge.merge(
        { generation: "g1", elements: elementsFor(450, "t"), frameOwners: [] },
        [],
        [],
        { supported: true, attached: 0 },
      );
      if (plain.elements.length !== 450 || plain.frames.length !== 0) {
        throw new Error("a frameless observation lost elements to the frame reservation");
      }
      if (plain.elements.some((element) => element.frameRef)) {
        throw new Error("a frameless observation gained frame provenance");
      }

      // An owner element that never produced its own target stays visible.
      const sameProcess = bridge.merge(
        {
          generation: "g1",
          elements: [],
          frameOwners: [
            { origin: "https://pay.example" },
            { origin: "https://top.example" },
            { origin: "https://ads.example" },
          ],
        },
        [{ ...frameResult(1, 1), urlOrigin: "https://pay.example" }],
        [{ urlOrigin: "https://blank.example", reason: "blank_document" }],
        { supported: true, attached: 1 },
      );
      const reasons = sameProcess.frameSummary.skipped.map((skip) => `${skip.urlOrigin}:${skip.reason}`);
      if (!reasons.includes("https://top.example:same_process_frame")
        || !reasons.includes("https://ads.example:same_process_frame")
        || reasons.includes("https://pay.example:same_process_frame")) {
        throw new Error(`same_process_frame reporting is wrong: ${JSON.stringify(reasons)}`);
      }
      if (!reasons.includes("https://blank.example:blank_document")) {
        throw new Error("session skips were dropped from the summary");
      }
      if (sameProcess.frameSummary.ownersSeen !== 3 || sameProcess.frameSummary.merged !== 1) {
        throw new Error("owner/merged counters are wrong");
      }

      // The skip report itself is bounded.
      const noisy = bridge.merge(
        {
          generation: "g1",
          elements: [],
          frameOwners: Array.from({ length: 40 }, (_, index) => ({ origin: `https://ad${index}.example` })),
        },
        [],
        [],
        { supported: true, attached: 0 },
      );
      if (noisy.frameSummary.skipped.length !== 32) {
        throw new Error(`skip report is unbounded: ${noisy.frameSummary.skipped.length}`);
      }
"##
        ),
    );
}

#[test]
fn frame_refs_fail_stale_with_coaching_before_any_lookup() {
    run_frame_harness(
        "frame ref harness",
        &format!(
            "{FRAME_HARNESS_PRELUDE}{}",
            r##"      const bridge = makeFrameBridge();
      await bridge.attach("child-1", { type: "iframe", targetId: "F1", url: "https://pay.example/" });
      await bridge.observe({ generation: "gen-b1", elements: [], frameOwners: [] });
      bridge.publishWatched("gen-b1");

      let staleMessage = null;
      try { bridge.resolve("gen-a0.f1.e1", "gen-b1"); }
      catch (error) { staleMessage = error.message; }
      if (staleMessage !== "STALE_REF: snapshot gen-a0 superseded by gen-b1; observe the page again and use fresh refs") {
        throw new Error(`a superseded frame ref lacked coaching: ${staleMessage}`);
      }
      if (bridge.lookups().length !== 0) {
        throw new Error("a superseded frame ref touched the frames map");
      }

      const resolved = bridge.resolve("gen-b1.f1.e1", "gen-b1");
      if (resolved.key !== "e1" || resolved.record.frameId !== "F1") {
        throw new Error("a fresh frame ref did not resolve to its own frame session");
      }
      if (bridge.lookups().join() !== "f1") {
        throw new Error(`a fresh frame ref did the wrong lookup: ${bridge.lookups().join()}`);
      }

      // A ref that is not the three-segment frame grammar never reaches a
      // frame session at all.
      for (const junk of ["e1", "gen-b1.e1", "gen-b1.f17.e1", "gen-b1.f1.e0"]) {
        if (bridge.parseRef(junk) !== null) throw new Error(`parsed a non-frame ref: ${junk}`);
      }
      if (bridge.parseRef("gen-b1.f16.e9999") === null) throw new Error("rejected a legal frame ref");

      // Non-pointer actions refuse a frame ref as a capability statement.
      let refusal = null;
      try { bridge.refuse("gen-b1.f1.e1", "page.fill"); } catch (error) { refusal = error.message; }
      if (!refusal?.startsWith("FRAME_ACTION_UNSUPPORTED: page.fill")) {
        throw new Error(`page.fill did not refuse a frame ref: ${refusal}`);
      }
      bridge.refuse("gen-b1.e1", "page.fill");
"##
        ),
    );
}

#[test]
fn frame_click_reruns_the_frame_proof_and_dispatches_translated_input() {
    run_frame_harness(
        "frame click harness",
        &format!(
            "{FRAME_HARNESS_PRELUDE}{}",
            r##"      const bridge = makeFrameBridge();
      await bridge.attach("child-1", { type: "iframe", targetId: "F1", url: "https://pay.example/" });
      await bridge.observe({ generation: "g1", elements: [], frameOwners: [] });
      bridge.publish("g1");

      const result = await bridge.click("g1.f1.e1", "g1");
      if (result.clicked !== true || result.trusted !== true) {
        throw new Error(`frame click did not complete: ${JSON.stringify(result)}`);
      }
      const order = bridge.order();
      const at = (needle) => {
        const index = order.findIndex((entry) => entry.includes(needle));
        if (index < 0) throw new Error(`missing step ${needle}: ${order.join(" | ")}`);
        return index;
      };
      const sequence = [
        "content:assertGeneration",
        "document:frame click preparation",
        "lease:frame click preparation frame precheck",
        "pointer:",
        "lease:click target commit",
        "lease:frame click commit frame precheck",
        "document:trusted click commit",
        "held:persist",
        "input:mousePressed",
        "input:mouseReleased",
        "held:clear",
        "document:trusted click completion",
      ];
      for (let index = 1; index < sequence.length; index += 1) {
        if (at(sequence[index - 1]) >= at(sequence[index])) {
          throw new Error(`step ${sequence[index]} did not follow ${sequence[index - 1]}: ${order.join(" | ")}`);
        }
      }

      // The frame proof runs twice: once to prepare and once immediately
      // before the agent is allowed to commit.
      const agentCalls = bridge.calls.filter((call) => call.agent).map((call) => call.agent.method);
      if (agentCalls.join(",") !== "snapshot,prepareClick,commitClick") {
        throw new Error(`frame agent call order is wrong: ${agentCalls.join(",")}`);
      }
      const ownerProbes = bridge.calls.filter((call) => call.method === "Runtime.callFunctionOn").length;
      if (ownerProbes !== 2) {
        throw new Error(`the owner hit test did not run exactly at prepare and commit: ${ownerProbes}`);
      }

      // Input is dispatched on the ROOT session at the translated centre.
      const dispatched = bridge.calls.filter((call) => call.method === "Input.dispatchMouseEvent");
      if (dispatched.length !== 2) throw new Error("frame click dispatched the wrong number of pointer events");
      for (const call of dispatched) {
        if (call.sessionId !== null && call.sessionId !== undefined) {
          throw new Error("pointer input was dispatched on a child session");
        }
        if (call.params.x !== 145 || call.params.y !== 79) {
          throw new Error(`pointer input was not at the translated centre: ${JSON.stringify(call.params)}`);
        }
      }
      if (!order.includes("pointer:145,79")) {
        throw new Error(`the virtual cursor did not move to the translated centre: ${order.join(" | ")}`);
      }

      // No request the frame agent ever receives carries a top-level
      // coordinate: it only ever sees its own frame-local geometry.
      for (const call of bridge.calls.filter((entry) => entry.agent)) {
        const payload = JSON.stringify(call.agent);
        for (const topLevel of ["125", "145", "69", "79"]) {
          if (payload.includes(topLevel)) {
            throw new Error(`a frame agent request carried a top-level coordinate: ${payload}`);
          }
        }
        for (const key of Object.keys(call.agent)) {
          if (!["method", "nonce", "key", "agentGeneration", "limit", "proof"].includes(key)) {
            throw new Error(`unexpected frame agent request field: ${key}`);
          }
        }
      }
"##
        ),
    );
}

#[test]
fn frame_staleness_fails_closed_before_and_after_dispatch() {
    run_frame_harness(
        "frame staleness harness",
        &format!(
            "{FRAME_HARNESS_PRELUDE}{}",
            r##"      async function armedBridge(world, options = {}) {
        const bridge = makeFrameBridge(world, options);
        await bridge.attach("child-1", { type: "iframe", targetId: "F1", url: "https://pay.example/" });
        await bridge.observe({ generation: "g1", elements: [], frameOwners: [] });
        bridge.publish("g1");
        return bridge;
      }
      const afterPrepare = (mutate) => (method, params, sessionId, api) => {
        if (method === "Runtime.evaluate" && String(params.expression).includes('"prepareClick"')) {
          mutate(api);
        }
      };
      async function failedClick(world, options) {
        const bridge = await armedBridge(world, options);
        let failure = null;
        try { await bridge.click("g1.f1.e1", "g1"); }
        catch (error) { failure = error; }
        if (!failure) throw new Error("a stale frame click was allowed to complete");
        return { bridge, failure };
      }

      // (a) The frame detaches between prepare and commit.
      {
        const { bridge, failure } = await failedClick(
          defaultWorld(),
          { afterCall: afterPrepare((api) => { api.records()[0].detached = true; }) },
        );
        if (failure.code !== "FRAME_DETACHED") throw new Error(`detach was not FRAME_DETACHED: ${failure.message}`);
        if (bridge.order().some((step) => step.startsWith("input:"))) {
          throw new Error("a detached frame still dispatched pointer input");
        }
      }

      // (b) The frame navigated: same frame id, new loader.
      {
        const world = defaultWorld();
        const { bridge, failure } = await failedClick(world, {
          afterCall: afterPrepare(() => { world.frames["child-1"].tree.frame.loaderId = "L2"; }),
        });
        if (failure.code !== "FRAME_DETACHED") throw new Error(`loader change was not FRAME_DETACHED: ${failure.message}`);
        if (bridge.order().some((step) => step.startsWith("input:"))) {
          throw new Error("a renavigated frame still dispatched pointer input");
        }
      }

      // (c) The owner box moved further than the geometry tolerance.
      {
        const world = defaultWorld();
        const { bridge, failure } = await failedClick(world, {
          afterCall: afterPrepare(() => { world.frames["child-1"].owner.x = 126; }),
        });
        if (failure.code !== "STALE_FRAME_TREE") throw new Error(`owner movement was not STALE_FRAME_TREE: ${failure.message}`);
        if (bridge.order().some((step) => step.startsWith("input:"))) {
          throw new Error("a moved frame owner still dispatched pointer input");
        }
      }

      // (d) The owner element is no longer the top hit at the translated point.
      {
        const world = defaultWorld();
        const { bridge, failure } = await failedClick(world, {
          afterCall: afterPrepare(() => { world.ownerHit = false; }),
        });
        if (!failure.message.startsWith("TARGET_OCCLUDED:")) {
          throw new Error(`an occluded frame owner was not refused: ${failure.message}`);
        }
        if (bridge.order().some((step) => step.startsWith("input:"))) {
          throw new Error("an occluded frame owner still dispatched pointer input");
        }
      }

      // A movement inside the tolerance is still allowed: the check is a
      // geometry bound, not an exact-equality trap.
      {
        const world = defaultWorld();
        const bridge = await armedBridge(world, {
          afterCall: afterPrepare(() => {
            world.frames["child-1"].owner.x = 121;
            world.frames["child-1"].owner.measuredX = 121;
          }),
        });
        const tolerated = await bridge.click("g1.f1.e1", "g1");
        if (tolerated.clicked !== true) throw new Error("a sub-tolerance owner shift blocked a legitimate click");
      }

      // (e) The document changes after mousePressed: the outcome is unknown
      // and the held release still fires.
      {
        const { bridge, failure } = await failedClick(
          defaultWorld(),
          { documentFailsAfter: "mouse release" },
        );
        if (failure.code !== "ACTION_OUTCOME_UNKNOWN") {
          throw new Error(`a post-press change was not ACTION_OUTCOME_UNKNOWN: ${failure.message}`);
        }
        const order = bridge.order();
        if (!order.includes("input:mousePressed")) throw new Error("the press never happened in case (e)");
        if (!order.includes("held:release")) throw new Error("the held mouse release was not fired after an unknown outcome");
      }

      // A frame that navigated before the action is refused before any CDP
      // work at all.
      {
        const bridge = await armedBridge(defaultWorld());
        bridge.event("child-1", "Page.frameNavigated", { frame: { id: "F1" } });
        let failure = null;
        try { await bridge.click("g1.f1.e1", "g1"); } catch (error) { failure = error; }
        if (failure?.message !== "STALE_SNAPSHOT: the frame tree changed: frame_navigated; observe the page again before acting") {
          throw new Error(`a navigated frame was not refused with coaching: ${failure && failure.message}`);
        }
      }

      // (f) Depth 2: the ANCESTOR frame moves between prepare and commit.
      // The target owner is measured inside that ancestor and does not move
      // at all, and the accumulated offset is built from remembered ancestor
      // origins, so only re-measuring the ancestor can catch the shift. This
      // is the difference between a refusal and a click at a stale point.
      function nestedWorld() {
        return {
          routeStripsSession: false,
          ownerHit: true,
          rootTree: {
            frame: { id: "ROOT", loaderId: "L0", url: "https://top.example/" },
            childFrames: [{ frame: { id: "F1", parentId: "ROOT", url: "https://pay.example/" } }],
          },
          frames: {
            "child-1": {
              targetId: "F1",
              tree: {
                frame: { id: "F1", parentId: "ROOT", loaderId: "L1", url: "https://pay.example/" },
                childFrames: [{ frame: { id: "F2", parentId: "F1", url: "https://deep.example/" } }],
              },
              owner: { backendNodeId: 501, x: 120, y: 64, width: 380, height: 220 },
              elements: [],
            },
            "child-2": {
              targetId: "F2",
              tree: { frame: { id: "F2", parentId: "F1", loaderId: "L2", url: "https://deep.example/" } },
              owner: { backendNodeId: 502, x: 10, y: 10, width: 200, height: 120 },
              elements: [{
                key: "e1", role: "button", name: "Inner", type: "submit", disabled: false,
                sensitive: false, inViewport: true, bounds: { x: 5, y: 5, width: 40, height: 20 },
                proof: { signature: "sig-2", bounds: { x: 5, y: 5, width: 40, height: 20 } },
              }],
            },
          },
        };
      }
      async function armedNested(world, options = {}) {
        const bridge = makeFrameBridge(world, options);
        await bridge.attach("child-2", { type: "iframe", targetId: "F2", url: "https://deep.example/" });
        await bridge.attach("child-1", { type: "iframe", targetId: "F1", url: "https://pay.example/" });
        await bridge.observe({ generation: "g1", elements: [], frameOwners: [] });
        bridge.publish("g1");
        return bridge;
      }
      {
        const world = nestedWorld();
        const bridge = await armedNested(world, {
          afterCall: afterPrepare(() => { world.frames["child-1"].owner.y = 164; }),
        });
        let failure = null;
        try { await bridge.click("g1.f2.e1", "g1"); } catch (error) { failure = error; }
        if (failure?.code !== "STALE_FRAME_TREE" || !/an ancestor frame owner element moved/.test(failure.message)) {
          throw new Error(`a moved ancestor frame was not refused: ${failure && failure.message}`);
        }
        if (bridge.order().some((step) => step.startsWith("input:"))) {
          throw new Error("a moved ancestor frame still dispatched pointer input");
        }
      }
      {
        // The same depth-2 click with nothing moving still completes, at the
        // point accumulated through both boundaries: 120+64 plus 10+10 plus
        // the frame-local centre of the target.
        const bridge = await armedNested(nestedWorld());
        const clicked = await bridge.click("g1.f2.e1", "g1");
        if (clicked.clicked !== true) throw new Error("a healthy depth-2 frame click was refused");
        const dispatched = bridge.calls.filter((call) => call.method === "Input.dispatchMouseEvent");
        for (const call of dispatched) {
          if (call.params.x !== 155 || call.params.y !== 89) {
            throw new Error(`a depth-2 click was dispatched at the wrong point: ${JSON.stringify(call.params)}`);
          }
        }
        // The ancestor owner is re-measured on every proof, not only the
        // target owner: two frames times prepare and commit.
        const ownerMeasurements = bridge.calls.filter((call) => call.method === "DOM.getBoxModel").length;
        if (ownerMeasurements < 6) {
          throw new Error(`the ancestor owner was not re-measured on every proof: ${ownerMeasurements}`);
        }
      }

      // A real iframe has a border, and getBoundingClientRect reports the
      // border box while the coordinate chain uses the content box. The proof
      // compares like with like, so a border wider than the 2px tolerance is
      // not mistaken for movement and the click still happens at the
      // content-box centre.
      {
        const world = defaultWorld();
        world.frames["child-1"].owner.border = 6;
        const bridge = await armedBridge(world);
        const clicked = await bridge.click("g1.f1.e1", "g1");
        if (clicked.clicked !== true) throw new Error("a bordered iframe refused a legitimate frame click");
        for (const call of bridge.calls.filter((entry) => entry.method === "Input.dispatchMouseEvent")) {
          if (call.params.x !== 145 || call.params.y !== 79) {
            throw new Error(`a bordered iframe moved the click point: ${JSON.stringify(call.params)}`);
          }
        }
      }

      // An owner whose box model carries no border quad cannot be verified
      // against a border-box rect at all, so it fails closed instead of
      // being compared against the wrong box.
      {
        const world = defaultWorld();
        world.frames["child-1"].owner.noBorderQuad = true;
        const bridge = await armedBridge(world);
        let failure = null;
        try { await bridge.click("g1.f1.e1", "g1"); } catch (error) { failure = error; }
        if (failure?.code !== "STALE_FRAME_TREE" || !/no border box to verify/.test(failure.message)) {
          throw new Error(`an unmeasurable owner border box was not refused: ${failure && failure.message}`);
        }
        if (bridge.order().some((step) => step.startsWith("input:"))) {
          throw new Error("an unverifiable frame owner still dispatched pointer input");
        }
      }
"##
        ),
    );
}

#[test]
fn frame_tree_changes_invalidate_the_snapshot_like_a_mutation() {
    run_frame_harness(
        "frame invalidation harness",
        &format!(
            "{FRAME_HARNESS_PRELUDE}{}",
            r##"      async function armed() {
        const bridge = makeFrameBridge();
        await bridge.attach("child-1", { type: "iframe", targetId: "F1", url: "https://pay.example/" });
        await bridge.observe({ generation: "g1", elements: [], frameOwners: [] });
        bridge.publish("g1");
        bridge.fresh("g1");
        return bridge;
      }

      const cases = [
        ["frame_attached", async (bridge) => {
          await bridge.attach("child-2", { type: "iframe", targetId: "F2", url: "https://ads.example/" });
        }],
        ["frame_detached", async (bridge) => {
          bridge.event("child-1", "Page.frameDetached", { frameId: "F1" });
        }],
        ["frame_navigated", async (bridge) => {
          bridge.event("child-1", "Page.frameNavigated", { frame: { id: "F1", url: "https://evil.example/" } });
        }],
        ["frame_target_detached", async (bridge) => {
          bridge.rootEvent("Target.detachedFromTarget", { sessionId: "child-1" });
        }],
        ["frame_resized", async (bridge) => {
          bridge.rootEvent("Page.frameResized", {});
        }],
      ];
      for (const [reason, mutate] of cases) {
        const bridge = await armed();
        const before = bridge.revision();
        await mutate(bridge);
        if (bridge.revision() !== before + 1) {
          throw new Error(`${reason} did not bump the frame tree revision`);
        }
        let message = null;
        try { bridge.fresh("g1"); } catch (error) { message = error.message; }
        if (message !== `STALE_SNAPSHOT: the frame tree changed: ${reason}; observe the page again before acting`) {
          throw new Error(`${reason} did not coach the next action: ${message}`);
        }
      }

      // A navigated frame drops its agent handles, so nothing can be sent into
      // the old document even if the revision check were bypassed.
      const navigated = await armed();
      navigated.event("child-1", "Page.frameNavigated", { frame: { id: "F1" } });
      const record = navigated.records()[0];
      if (!record.navigated || record.worldContextId !== null || record.agentNonce !== "") {
        throw new Error("a navigated frame kept its isolated world or nonce");
      }

      // An unrelated child session's navigation never touches the lease.
      const unrelated = await armed();
      const revisionBefore = unrelated.revision();
      unrelated.event("child-unknown", "Page.frameNavigated", { frame: { id: "F9" } });
      if (unrelated.revision() !== revisionBefore) {
        throw new Error("an unknown child session invalidated the snapshot");
      }

      // Teardown clears every frame session at the single choke point.
      const torn = await armed();
      torn.clear();
      if (torn.records().length !== 0 || torn.snapshot() !== null || torn.support().supported !== false) {
        throw new Error("lease teardown left frame state behind");
      }
      let afterTeardown = null;
      try { torn.fresh("g1"); } catch (error) { afterTeardown = error.message; }
      if (afterTeardown !== "STALE_SNAPSHOT: observe the page again before acting") {
        throw new Error(`teardown did not fail closed: ${afterTeardown}`);
      }
"##
        ),
    );
}

#[test]
fn frame_agent_proofs_run_in_frame_local_coordinates() {
    run_frame_harness(
        "frame agent proof harness",
        &format!(
            "{FRAME_AGENT_HARNESS_PRELUDE}{}",
            r##"      const agent = makeAgent([
        { tagName: "BUTTON", text: "Pay now", rect: { x: 5, y: 5, width: 40, height: 20 } },
        { tagName: "A", text: "Terms", attributes: { href: "https://pay.example/terms" }, rect: { x: 5, y: 40, width: 60, height: 18 } },
      ]);

      if (agent.call({ method: "snapshot" }).error !== "FRAME_AGENT_STALE: this frame agent belongs to an older control lease") {
        throw new Error("an uninstalled agent answered a snapshot");
      }
      const installed = agent.call({ method: "install" });
      if (installed.ok !== true || !installed.result.nonce) throw new Error("install did not mint a lease nonce");
      const nonce = installed.result.nonce;

      const snapshot = agent.call({ method: "snapshot", nonce, limit: 120 });
      if (snapshot.ok !== true) throw new Error(`snapshot failed: ${snapshot.error}`);
      const first = snapshot.result.elements[0];
      if (JSON.stringify(first.bounds) !== JSON.stringify({ x: 5, y: 5, width: 40, height: 20 })) {
        throw new Error(`snapshot bounds are not frame-local: ${JSON.stringify(first.bounds)}`);
      }
      if (first.key !== "e1" || first.ref !== "e1") {
        throw new Error("the frame agent minted a ref instead of a frame-local key");
      }
      if (snapshot.result.total !== 2 || snapshot.result.truncated !== false) {
        throw new Error("snapshot totals are wrong");
      }
      const agentGeneration = snapshot.result.agentGeneration;

      const prepared = agent.call({ method: "prepareClick", nonce, key: "e1", agentGeneration });
      if (prepared.ok !== true) throw new Error(`prepareClick failed: ${prepared.error}`);
      const lastHit = agent.hitTests().at(-1);
      if (lastHit.x !== 25 || lastHit.y !== 15) {
        throw new Error(`the hit test did not use the frame-local centre: ${JSON.stringify(lastHit)}`);
      }
      if (JSON.stringify(prepared.result.proof.bounds) !== JSON.stringify({ x: 5, y: 5, width: 40, height: 20 })) {
        throw new Error("the proof is not frame-local");
      }

      // A stale lease nonce refuses everything.
      const stale = agent.call({ method: "prepareClick", nonce: "other", key: "e1", agentGeneration });
      if (stale.ok !== false || stale.error !== "FRAME_AGENT_STALE: this frame agent belongs to an older control lease") {
        throw new Error(`a stale nonce was accepted: ${JSON.stringify(stale)}`);
      }

      // A stale agent generation refuses before any element lookup.
      const staleGeneration = agent.call({ method: "prepareClick", nonce, key: "e1", agentGeneration: "older" });
      if (staleGeneration.error !== "STALE_SNAPSHOT: observe the page again before acting") {
        throw new Error(`a stale agent generation was accepted: ${JSON.stringify(staleGeneration)}`);
      }

      // Identity change.
      agent.elements[0].attributes.name = "renamed";
      const renamed = agent.call({ method: "prepareClick", nonce, key: "e1", agentGeneration });
      if (renamed.error !== "TARGET_CHANGED: target identity changed; observe again") {
        throw new Error(`a renamed target was accepted: ${JSON.stringify(renamed)}`);
      }
      delete agent.elements[0].attributes.name;

      // Geometry change.
      agent.elements[0].rect = { x: 5, y: 60, width: 40, height: 20 };
      const moved = agent.call({ method: "prepareClick", nonce, key: "e1", agentGeneration });
      if (moved.error !== "TARGET_CHANGED: target geometry changed; observe again") {
        throw new Error(`a moved target was accepted: ${JSON.stringify(moved)}`);
      }
      agent.elements[0].rect = { x: 5, y: 5, width: 40, height: 20 };

      // A commit whose proof does not match the live element is refused.
      const forged = agent.call({
        method: "commitClick", nonce, key: "e1", agentGeneration,
        proof: { signature: "forged", bounds: { x: 5, y: 5, width: 40, height: 20 } },
      });
      if (forged.error !== "TARGET_CHANGED: target proof no longer matches; observe again") {
        throw new Error(`a forged commit proof was accepted: ${JSON.stringify(forged)}`);
      }
      const committed = agent.call({ method: "commitClick", nonce, key: "e1", agentGeneration, proof: prepared.result.proof });
      if (committed.ok !== true || committed.result.validated !== true) {
        throw new Error(`a matching commit proof was refused: ${JSON.stringify(committed)}`);
      }

      // Unknown methods and disconnected elements answer, never throw.
      if (agent.call({ method: "nonsense", nonce }).error !== "FRAME_AGENT_FAILED: unknown frame agent request") {
        throw new Error("an unknown method did not fail closed");
      }
      agent.elements[0].isConnected = false;
      if (agent.call({ method: "describe", nonce, key: "e1", agentGeneration }).error !== "STALE_REF: the element changed; observe the page again") {
        throw new Error("a disconnected element did not fail closed");
      }
      if (agent.call(undefined).ok !== false) throw new Error("an empty request threw instead of answering");

      // Reinstalling re-keys the agent and drops every ref.
      const reinstalled = agent.call({ method: "install" });
      if (reinstalled.result.nonce === nonce) throw new Error("install reused the previous lease nonce");
      if (agent.call({ method: "prepareClick", nonce: reinstalled.result.nonce, key: "e1", agentGeneration }).error
        !== "STALE_SNAPSHOT: observe the page again before acting") {
        throw new Error("a reinstalled agent still answered an old snapshot generation");
      }
"##
        ),
    );
}

/// Every observation re-evaluates the agent source into the SAME isolated
/// world, so building a second agent would leak a whole-document
/// MutationObserver plus a scroll and a resize listener per observation, on
/// exactly the third-party iframes this feature targets.
#[test]
fn frame_agent_installs_exactly_one_observer_per_isolated_world() {
    let agent = extension_source("frame-agent.js");
    let builder = agent
        .find("globalThis.__LBB_FRAME_AGENT_SOURCE__ = () =>")
        .expect("the installable frame source builder is gone");
    let line_end = agent[builder..].find('\n').unwrap() + builder;
    assert!(
        agent[builder..line_end].contains("${installFrameAgentOnce}\\ninstallFrameAgentOnce();"),
        "the evaluated frame source no longer installs through the once guard"
    );
    assert!(agent.contains("if (!globalThis.__LBB_FRAME_AGENT__) {"));

    run_frame_harness(
        "frame agent install harness",
        &format!(
            "{FRAME_AGENT_HARNESS_PRELUDE}{}",
            r##"      const world = makeInstaller();
      const first = world.install();
      if (!first || typeof first.call !== "function") throw new Error("the frame agent did not install");
      if (world.observers() !== 1 || world.listeners().join(",") !== "scroll,resize") {
        throw new Error(`the first install did not register exactly one tracker: ${world.observers()} ${world.listeners()}`);
      }
      for (let index = 0; index < 30; index += 1) world.install();
      if (world.observers() !== 1) {
        throw new Error(`re-evaluating the frame source leaked observers: ${world.observers()}`);
      }
      if (world.listeners().length !== 2) {
        throw new Error(`re-evaluating the frame source leaked listeners: ${world.listeners().join(",")}`);
      }
      if (world.agent() !== first) throw new Error("a second agent replaced the installed one");

      // Re-keying per lease stays the job of the install request, which the
      // worker sends right after evaluating the source.
      const before = first.call({ method: "install" }).result.nonce;
      const after = first.call({ method: "install" }).result.nonce;
      if (!before || !after || before === after) {
        throw new Error("the reused agent stopped re-keying per lease");
      }
"##
        ),
    );
}

#[test]
fn frame_agent_refuses_after_its_own_document_mutates() {
    run_frame_harness(
        "frame agent freshness harness",
        &format!(
            "{FRAME_AGENT_HARNESS_PRELUDE}{}",
            r##"      const agent = makeAgent([
        { tagName: "BUTTON", text: "Pay now", rect: { x: 5, y: 5, width: 40, height: 20 } },
      ]);
      const nonce = agent.call({ method: "install" }).result.nonce;
      const agentGeneration = agent.call({ method: "snapshot", nonce }).result.agentGeneration;
      if (agent.call({ method: "prepareClick", nonce, key: "e1", agentGeneration }).ok !== true) {
        throw new Error("a fresh frame snapshot was refused");
      }

      agent.mutate();
      const afterMutation = agent.call({ method: "prepareClick", nonce, key: "e1", agentGeneration });
      if (afterMutation.error !== "STALE_SNAPSHOT: the document mutated; observe the page again before acting") {
        throw new Error(`a mutated frame document was not refused: ${JSON.stringify(afterMutation)}`);
      }
      const commitAfterMutation = agent.call({
        method: "commitClick", nonce, key: "e1", agentGeneration,
        proof: { signature: "x", bounds: { x: 5, y: 5, width: 40, height: 20 } },
      });
      if (commitAfterMutation.error !== "STALE_SNAPSHOT: the document mutated; observe the page again before acting") {
        throw new Error("a mutated frame document still accepted a commit");
      }

      // A scroll inside the frame invalidates the frame snapshot too.
      const rescanned = agent.call({ method: "snapshot", nonce }).result.agentGeneration;
      agent.scroll();
      const afterScroll = agent.call({ method: "prepareClick", nonce, key: "e1", agentGeneration: rescanned });
      if (afterScroll.error !== "STALE_SNAPSHOT: the page scrolled; observe the page again before acting") {
        throw new Error(`a scrolled frame was not refused: ${JSON.stringify(afterScroll)}`);
      }
"##
        ),
    );
}

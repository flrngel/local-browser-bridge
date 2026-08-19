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
    "lib.js",
    "manifest.json",
    "popup.css",
    "popup.html",
    "popup.js",
];

fn manifest() -> Value {
    serde_json::from_str(&fs::read_to_string("extension/manifest.json").unwrap()).unwrap()
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
fn manifest_has_only_reviewed_capabilities() {
    let manifest = manifest();
    assert_eq!(manifest["manifest_version"], 3);
    assert_eq!(manifest["version"], VERSION);
    assert_eq!(manifest["minimum_chrome_version"], "118");
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
    let popup = fs::read_to_string("extension/popup.html").unwrap();
    assert!(popup.contains("href=\"popup.css\""));
    assert!(popup.contains("src=\"popup.js\""));
}

#[test]
fn extension_executes_no_remote_code_or_update_client() {
    let source = EXTENSION_FILES
        .iter()
        .filter(|file| file.ends_with(".js") || file.ends_with(".html"))
        .map(|file| fs::read_to_string(Path::new("extension").join(file)).unwrap())
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
    let background = fs::read_to_string("extension/background.js").unwrap();
    assert!(background.contains("new WebSocket(`ws://127.0.0.1:"));
    assert!(!background.contains("wss://"));
}

#[test]
fn extension_allows_only_the_demo_on_the_bridge_origin() {
    let library = fs::read_to_string("extension/lib.js").unwrap();
    assert!(library.contains("bridgeOrigin && url.pathname !== \"/demo\""));
    assert!(library.contains("The bridge cannot control its own control surface"));
    assert!(!library.contains("url.pathname.startsWith(\"/api\")"));
}

#[test]
fn observations_are_non_activating_bounded_and_composed() {
    let background = fs::read_to_string("extension/background.js").unwrap();
    let content = fs::read_to_string("extension/content.js").unwrap();
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
    assert!(content.contains("element.shadowRoot"));
    assert!(content.contains("tree: element.getRootNode() instanceof ShadowRoot"));
}

#[test]
fn direct_input_requires_a_fresh_snapshot() {
    let background = fs::read_to_string("extension/background.js").unwrap();
    let content = fs::read_to_string("extension/content.js").unwrap();
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
    let background = fs::read_to_string("extension/background.js").unwrap();
    let content = fs::read_to_string("extension/content.js").unwrap();
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
    let background = fs::read_to_string("extension/background.js").unwrap();
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
fn control_lease_is_owned_by_the_authenticated_server_session() {
    let background = fs::read_to_string("extension/background.js").unwrap();
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
    let background = fs::read_to_string("extension/background.js").unwrap();
    let content = fs::read_to_string("extension/content.js").unwrap();

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
    assert!(content.contains("sameBounds"));
    assert!(content.contains("deepElementFromPoint"));
    assert!(content.contains("TARGET_OCCLUDED"));
    assert!(content.contains("TARGET_CHANGED"));
}

#[test]
fn snapshots_invalidate_on_mutation_scroll_and_resize() {
    let content = fs::read_to_string("extension/content.js").unwrap();
    assert!(content.contains("new MutationObserver"));
    assert!(content.contains("the document mutated"));
    assert!(content.contains("the page scrolled"));
    assert!(content.contains("the viewport resized"));
    assert!(content.contains("snapshotRevision !== documentRevision"));
    assert!(content.contains("snapshotInvalidated: true"));
}

#[test]
fn snapshots_and_target_proofs_never_embed_live_text_input_values() {
    let content = fs::read_to_string("extension/content.js").unwrap();
    let background = fs::read_to_string("extension/background.js").unwrap();
    assert!(content.contains("const safeInputValue = element instanceof HTMLInputElement"));
    assert!(content.contains("[\"button\", \"submit\", \"reset\"].includes(element.type)"));
    assert!(
        content.contains(
            "element instanceof HTMLInputElement || element instanceof HTMLTextAreaElement"
        )
    );
    assert!(content.contains("isSensitiveFieldMetadata({ type, autocomplete, name: fieldName })"));
    assert!(!content.contains("![\"password\", \"hidden\"].includes(element.type)"));
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
      const source = fs.readFileSync("extension/content.js", "utf8");
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
    let background = fs::read_to_string("extension/background.js").unwrap();
    let content = fs::read_to_string("extension/content.js").unwrap();
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
    let content = fs::read_to_string("extension/content.js").unwrap();
    let popup = fs::read_to_string("extension/popup.html").unwrap();
    let popup_script = fs::read_to_string("extension/popup.js").unwrap();

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
    let background = fs::read_to_string("extension/background.js").unwrap();
    let content = fs::read_to_string("extension/content.js").unwrap();
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
    let background = fs::read_to_string("extension/background.js").unwrap();
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
fn timeout_cancel_is_session_bound_and_suppresses_late_results() {
    let background = fs::read_to_string("extension/background.js").unwrap();
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
    assert!(cancel.contains("cancelCommandContext(context, \"server_timeout\")"));
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
    assert!(chain.contains("activeCommandContexts.delete(context.key)"));
    assert!(background.contains("context.cancelWaiters.add(rejectCancellation)"));
    assert!(background.contains("for (const reject of context.cancelWaiters ?? [])"));
}

#[test]
fn renderer_and_debugger_lifecycle_waits_are_bounded_and_cancel_aware() {
    let background = fs::read_to_string("extension/background.js").unwrap();
    let content = fs::read_to_string("extension/content.js").unwrap();

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
    let background = fs::read_to_string("extension/background.js").unwrap();
    let content = fs::read_to_string("extension/content.js").unwrap();
    assert!(background.contains("let controlEpoch = 0"));
    assert!(background.contains("controlEpoch += 1"));
    assert!(background.contains("controlLease.epoch !== authority.epoch"));
    assert!(background.contains("controlEpoch !== authority.epoch"));
    assert!(background.contains("async function leaseSideEffect(authority, context"));
    assert!(
        background
            .contains("controlEpoch: controlLease?.tabId === tabId ? controlLease.epoch : null")
    );
    assert!(
        background
            .contains("async function dispatch(method, params, approved, commandContext = null)")
    );

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
    let background = fs::read_to_string("extension/background.js").unwrap();
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
    let background = fs::read_to_string("extension/background.js").unwrap();
    let popup = fs::read_to_string("extension/popup.js").unwrap();
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
    let background = fs::read_to_string("extension/background.js").unwrap();
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
    let background = fs::read_to_string("extension/background.js").unwrap();
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
    let background = fs::read_to_string("extension/background.js").unwrap();
    let popup = fs::read_to_string("extension/popup.js").unwrap();
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
    let background = fs::read_to_string("extension/background.js").unwrap();
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
        ("tabs.new", "chrome.tabs.create"),
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
    let background = fs::read_to_string("extension/background.js").unwrap();
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
    let background = fs::read_to_string("extension/background.js").unwrap();
    let content = fs::read_to_string("extension/content.js").unwrap();

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
    let background = fs::read_to_string("extension/background.js").unwrap();
    let library = fs::read_to_string("extension/lib.js").unwrap();
    assert!(library.contains("fullAccess || trustedBlank"));
    assert!(library.contains("Untracked blank tabs are blocked in Safe mode"));
    assert!(background.contains("bridgeCreatedTabs.has(tab.id)"));
    assert!(background.contains("!Number.isInteger(tab.openerTabId)"));
    assert!(background.contains("tab?.pendingUrl || tab?.url"));
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
    let background = fs::read_to_string("extension/background.js").unwrap();
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
        ${functions}
        return {
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
    let background = fs::read_to_string("extension/background.js").unwrap();
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
    let background = fs::read_to_string("extension/background.js").unwrap();
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
    let background = fs::read_to_string("extension/background.js").unwrap();
    let library = fs::read_to_string("extension/lib.js").unwrap();
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
fn server_and_extension_command_allowlists_match() {
    let background = fs::read_to_string("extension/background.js").unwrap();
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

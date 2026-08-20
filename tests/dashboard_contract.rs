use std::fs;
use std::process::Command;

#[test]
fn dashboard_requires_monotonic_state_and_exact_decoded_frame_images() {
    let app = fs::read_to_string("public/app.js").unwrap();
    assert!(app.contains("function shouldAcceptStateRevision"));
    assert!(app.contains("nextRevision < currentRevision"));
    assert!(app.contains("const imageLoadStates = new WeakMap()"));
    assert!(app.contains("const controller = new AbortController()"));
    assert!(app.contains("signal: controller.signal"));
    assert!(app.contains("await element.decode()"));
    assert!(app.contains("imageLoadStates.get(element) !== requestState"));
    assert!(app.contains("protectedImageReady(ui[\"computer-screenshot\"]"));
    assert!(app.contains("currentState.browserControl?.humanPaused === true"));
    assert!(app.contains("Only Resume remote control in the extension popup"));
    assert!(app.contains("Resume in extension popup"));
    assert!(app.contains("Sensitive value redacted"));
    assert!(app.contains("!element.sensitive && !element.valueRedacted"));
}

#[test]
fn dashboard_describes_native_capture_evidence_and_development_builds_honestly() {
    let app = fs::read_to_string("public/app.js").unwrap();
    let page = fs::read_to_string("public/index.html").unwrap();
    let security = fs::read_to_string("SECURITY.md").unwrap();
    let architecture = fs::read_to_string("docs/ARCHITECTURE.md").unwrap();
    let limitations = fs::read_to_string("docs/LIMITATIONS.md").unwrap();
    let sota_audit = fs::read_to_string("docs/SOTA_AUDIT.md").unwrap();

    assert!(app.contains("native input route unavailable"));
    assert!(!app.contains("input permission required"));
    assert!(app.contains("matching samples cannot rule out a shorter transient change"));
    assert!(!app.contains("stay untouched"));
    assert!(app.contains("development: `Development build ahead of stable"));
    assert!(app.contains("Recheck stable release"));
    assert!(app.contains("View latest stable release"));
    assert!(app.contains("failed: true"));
    assert!(app.contains("Previous result\", \"Cleared"));

    assert!(page.contains("One-shot observe"));
    assert!(page.contains("Start persistent share"));
    assert!(page.contains("Share keeps a persistent operating-system exact-window stream open"));
    assert!(page.contains("do not prove zero interruption"));
    assert!(!page.contains("prove non-interrupting delivery"));

    assert!(security.contains("best-effort target-routed input"));
    assert!(architecture.contains("shared-session operation with before/after invariant checks"));
    assert!(limitations.contains("macOS 14 and later can use the filter's point-to-pixel scale"));
    assert!(
        !limitations.contains("macOS 14.2 and later can use the filter's point-to-pixel scale")
    );
    assert!(sota_audit.contains("matching samples do not prove zero interruption"));
    assert!(!sota_audit.contains("This is shared-session non-interruption"));
}

#[test]
fn deferred_dashboard_responses_cannot_regress_state_or_replace_newer_pixels() {
    let script = r#"
      import fs from "node:fs";

      function extractFunction(source, name) {
        const markers = [`async function ${name}(`, `function ${name}(`];
        const start = markers.map((marker) => source.indexOf(marker)).find((index) => index >= 0);
        if (start === undefined) throw new Error(`missing ${name}`);
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
          else if (character === "}") {
            depth -= 1;
            if (depth === 0) return source.slice(start, index + 1);
          }
        }
        throw new Error(`unterminated ${name}`);
      }

      const source = fs.readFileSync("public/app.js", "utf8");
      const names = [
        "shouldAcceptStateRevision", "computerImageKey", "protectedImageReady", "loadProtectedImage",
      ];
      const functions = names.map((name) => extractFunction(source, name)).join("\n");
      const harness = new Function(`
        let sessionToken = "test-session";
        const imageObjectUrls = new WeakMap();
        const imageLoadStates = new WeakMap();
        const pending = new Map();
        const revoked = [];
        const URL = {
          createObjectURL(blob) { return \`blob:\${blob.label}\`; },
          revokeObjectURL(url) { revoked.push(url); },
        };
        const fetch = (path) => new Promise((resolve) => pending.set(path, resolve));
        const element = {
          src: "",
          setAttribute() {},
          removeAttribute() {},
          async decode() {},
        };
        const ui = { "computer-screenshot": element };
        function syncComputerAvailability() {}
        ${functions}
        return {
          shouldAcceptStateRevision,
          loadProtectedImage,
          protectedImageReady,
          pending,
          element,
          revoked,
        };
      `)();

      if (harness.shouldAcceptStateRevision({ revision: 9 }, { revision: 10 })) {
        throw new Error("older state revision was accepted");
      }
      if (!harness.shouldAcceptStateRevision({ revision: 11 }, { revision: 10 })) {
        throw new Error("newer state revision was rejected");
      }

      const oldLoad = harness.loadProtectedImage(harness.element, "/old", "old-key");
      const newLoad = harness.loadProtectedImage(harness.element, "/new", "new-key");
      harness.pending.get("/new")({ ok: true, status: 200, blob: async () => ({ label: "new" }) });
      if (!(await newLoad)) throw new Error("new image did not commit");
      harness.pending.get("/old")({ ok: true, status: 200, blob: async () => ({ label: "old" }) });
      if (await oldLoad) throw new Error("superseded image unexpectedly committed");
      if (harness.element.src !== "blob:new") throw new Error(`visible image regressed to ${harness.element.src}`);
      if (!harness.protectedImageReady(harness.element, "new-key")) {
        throw new Error("exact decoded image was not marked ready");
      }
      if (harness.protectedImageReady(harness.element, "old-key")) {
        throw new Error("superseded image remained action-authoritative");
      }
    "#;

    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run dashboard ordering harness: {error}"),
    };
    assert!(
        output.status.success(),
        "dashboard ordering harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn failed_native_mutation_replaces_prior_success_evidence() {
    let script = r#"
      import fs from "node:fs";

      const source = fs.readFileSync("public/app.js", "utf8");
      const runActionStart = source.indexOf("async function runAction(");
      const runActionEnd = source.indexOf("\nasync function checkForUpdate(", runActionStart);
      if (runActionStart < 0 || runActionEnd < 0) throw new Error("missing runAction");
      const runActionSource = source.slice(runActionStart, runActionEnd);
      const harness = new Function(`
        let busy = false;
        let lastComputerAction = {
          method: "computer.click",
          result: { effect: "Confirmed", actionId: "old-success" },
        };
        const COMPUTER_MUTATION_METHODS = new Set(["computer.click"]);
        const renderedEvidence = [];
        function setBusy(value) { busy = value; }
        async function request() {
          const error = new Error("dispatch refused");
          error.code = "COMPUTER_BACKGROUND_UNAVAILABLE";
          throw error;
        }
        function render() {}
        function renderComputerActionEvidence() {
          renderedEvidence.push(JSON.parse(JSON.stringify(lastComputerAction)));
        }
        function showToast() {}
        async function loadState() {}
        ${runActionSource}
        return {
          runAction,
          action: () => lastComputerAction,
          renderedEvidence,
          busy: () => busy,
        };
      `)();

      const result = await harness.runAction("computer.click", { frameId: "fresh-frame" });
      const action = harness.action();
      if (result !== null) throw new Error("failed action did not return null");
      if (!action.failed) throw new Error("failed action did not replace the prior success");
      if (action.result !== undefined) throw new Error("stale successful result was retained");
      if (action.error?.code !== "COMPUTER_BACKGROUND_UNAVAILABLE") {
        throw new Error(`wrong failure code: ${action.error?.code}`);
      }
      if (action.error?.message !== "dispatch refused") {
        throw new Error(`wrong failure message: ${action.error?.message}`);
      }
      if (harness.renderedEvidence.length !== 1 || !harness.renderedEvidence[0].failed) {
        throw new Error("explicit failed evidence was not rendered immediately");
      }
      if (harness.busy()) throw new Error("dashboard remained busy after failure");
    "#;

    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run native-action failure harness: {error}"),
    };
    assert!(
        output.status.success(),
        "native-action failure harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

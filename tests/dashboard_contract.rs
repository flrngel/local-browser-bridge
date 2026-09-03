use std::fs;
use std::process::Command;

#[test]
fn node_runtime_is_available_for_dashboard_behavior_contracts() {
    let output = Command::new("node")
        .arg("--version")
        .output()
        .expect("Node.js is a test-only dependency for dashboard behavior contracts; it is not an end-user runtime dependency");
    assert!(
        output.status.success(),
        "Node.js could not execute the dashboard behavior contracts: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

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

/// Extracts the contents of every `"..."`, `'...'`, and `` `...` `` literal in
/// `source`, joined by spaces. Used to jargon-check only the text a user can
/// actually see rendered, not surrounding JavaScript keywords/identifiers.
fn string_literals(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        let character = chars[index];
        if character == '"' || character == '\'' || character == '`' {
            let quote = character;
            index += 1;
            while index < chars.len() {
                let inner = chars[index];
                if inner == '\\' {
                    index += 2;
                    continue;
                }
                if inner == quote {
                    index += 1;
                    break;
                }
                out.push(inner);
                index += 1;
            }
            out.push(' ');
        } else {
            index += 1;
        }
    }
    out
}

#[test]
fn home_view_is_default_and_jargon_free_with_required_controls() {
    let page = fs::read_to_string("public/index.html").unwrap();
    let app = fs::read_to_string("public/app.js").unwrap();

    // Home is what loads by default; the entire old console is parked in a
    // hidden "Advanced controls" view instead of being deleted.
    assert!(page.contains("<div id=\"home-view\">"));
    assert!(page.contains("id=\"advanced-view\" hidden"));

    // The four required status rows and the "point your AI here" controls exist.
    assert!(page.contains("id=\"status-app-pill\""));
    assert!(page.contains("id=\"status-browser-pill\""));
    assert!(page.contains("id=\"status-browser-action\""));
    assert!(page.contains("id=\"status-desktop-toggle\""));
    assert!(page.contains("id=\"status-shell-toggle\""));
    assert!(page.contains("id=\"copy-ai-instructions\""));
    assert!(page.contains("id=\"copy-ai-link\""));

    // No developer jargon anywhere in the Home markup or its rendering code.
    let home_start = page
        .find("<!-- HOME:START -->")
        .expect("missing HOME:START marker in public/index.html");
    let home_end = page
        .find("<!-- HOME:END -->")
        .expect("missing HOME:END marker in public/index.html");
    let home_markup = page[home_start..home_end].to_lowercase();

    let js_start = app
        .find("// HOME:STRINGS:START")
        .expect("missing HOME:STRINGS:START marker in public/app.js");
    let js_end = app
        .find("// HOME:STRINGS:END")
        .expect("missing HOME:STRINGS:END marker in public/app.js");
    // Only the quoted string literals (what a user can actually see rendered
    // as text) are checked here, not the surrounding code -- otherwise
    // ordinary keywords/identifiers (e.g. `return`, `bridgeToken`) would
    // produce false positives against substrings like "turn" or "token".
    let home_js = string_literals(&app[js_start..js_end]).to_lowercase();

    let banned = [
        "lease",
        "loopback",
        "oopif",
        "capability",
        "generation",
        "turn",
        "postcondition",
        "invariant",
        "semantic ref",
        "ttl",
        "cdp",
        "handshake",
        "token",
        "master",
    ];
    for word in banned {
        assert!(
            !home_markup.contains(word),
            "banned word `{word}` found in Home markup (public/index.html)"
        );
        assert!(
            !home_js.contains(word),
            "banned word `{word}` found in Home rendering code (public/app.js)"
        );
    }
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
    assert!(app.contains("May use a transient target AXFrontmost lease"));
    assert!(!app.contains("stay untouched"));
    assert!(app.contains("development: `Development build ahead of stable"));
    assert!(app.contains("Recheck stable release"));
    assert!(app.contains("View latest stable release"));
    assert!(app.contains("failed: true"));
    assert!(app.contains("Previous result\", \"Cleared"));

    assert!(page.contains("One-shot observe"));
    assert!(page.contains("Start persistent share"));
    assert!(page.contains("Browser control lease"));
    assert!(!page.contains("Chrome control lease"));
    assert!(app.contains("state.extension?.browser"));
    assert!(app.contains("The browser's native debugging notice"));
    assert!(page.contains("Share keeps a persistent operating-system exact-window stream open"));
    assert!(page.contains("do not prove zero interruption"));
    assert!(page.contains("may briefly make the exact target <code>AXFrontmost</code>"));
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

/// Regression test for a real bug seen live: the server reports
/// `desktopControlSetup: { state: "ready", percent: 0 }` whenever nothing is
/// actually downloading (this is the *only* state macOS ever reports, since
/// it never downloads a helper at all), and the Screen control row rendered
/// that as a permanent, fabricated "Getting ready... 0%". This exercises the
/// real `renderHome` function from `public/app.js` against a fake DOM and
/// checks every `desktopControlSetup.state` the server can actually emit.
#[test]
fn screen_control_status_only_claims_progress_while_actually_downloading() {
    let script = r#"
      import fs from "node:fs";

      const source = fs.readFileSync("public/app.js", "utf8");
      const start = source.indexOf("function renderHome(state) {");
      const end = source.indexOf("\nui[\"show-advanced\"]", start);
      if (start < 0 || end < 0) throw new Error("missing renderHome in public/app.js");
      const renderHomeSource = source.slice(start, end);

      function makeElement() {
        return { hidden: false, textContent: "", className: "", checked: false, onclick: null };
      }

      const ids = [
        "status-app-pill", "status-browser-pill", "status-browser-action", "home-browser-setup",
        "status-desktop-toggle", "status-desktop-pill", "status-desktop-progress", "status-desktop-retry",
        "status-shell-toggle", "status-shell-pill", "home-hero-headline", "home-hero-action",
      ];

      const makeRenderHome = new Function("ui", `
        let connectAttemptFailed = false;
        function setStateBadge(element, label, tone) { element.textContent = label; element.className = tone; }
        function setTextIfChanged(element, value) { element.textContent = value; }
        function renderExtensionFolderPath() {}
        function connectExtension() {}
        function postSettings() {}
        ${renderHomeSource}
        return renderHome;
      `);

      function run(desktopControlSetup) {
        const ui = {};
        for (const id of ids) ui[id] = makeElement();
        const renderHome = makeRenderHome(ui);
        renderHome({
          setup: {
            browserConnected: true,
            desktopControlEnabled: true,
            desktopControlConnected: false,
            ready: false,
            desktopControlSetup,
          },
        });
        return ui;
      }

      // The exact live reproduction: server default/idle state, 0%, nothing
      // downloading, and nothing ever will be on macOS.
      const idle = run({ state: "ready", percent: 0, message: "" });
      if (idle["status-desktop-progress"].textContent.includes("%")) {
        throw new Error(`"ready" state fabricated a percent: ${idle["status-desktop-progress"].textContent}`);
      }
      if (idle["status-desktop-progress"].hidden) {
        throw new Error("\"ready\" state hid the status line; it must show a truthful sentence instead");
      }
      if (!idle["status-desktop-retry"].hidden) {
        throw new Error("\"ready\" (non-failed) state must not show Retry");
      }
      if (idle["home-hero-headline"].textContent.toLowerCase().includes("setting up")) {
        throw new Error(`hero headline still claims setup is in progress: ${idle["home-hero-headline"].textContent}`);
      }

      // A real download in progress must still show its real, live percent.
      const downloading = run({ state: "downloading", percent: 42, message: "Downloading the Computer Helper…" });
      if (!downloading["status-desktop-progress"].textContent.includes("42%")) {
        throw new Error(`"downloading" state lost its percent: ${downloading["status-desktop-progress"].textContent}`);
      }
      if (!downloading["home-hero-headline"].textContent.toLowerCase().includes("setting up")) {
        throw new Error("\"downloading\" state must still say setup is in progress on the hero headline");
      }

      // A failed setup must still show its reason and the Retry action.
      const failed = run({ state: "failed", percent: 0, message: "The download failed." });
      if (failed["status-desktop-retry"].hidden) {
        throw new Error("\"failed\" state must show Retry");
      }
      if (!failed["status-desktop-progress"].textContent.includes("The download failed.")) {
        throw new Error(`"failed" state lost its reason: ${failed["status-desktop-progress"].textContent}`);
      }
    "#;

    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run screen-control status harness: {error}"),
    };
    assert!(
        output.status.success(),
        "screen-control status harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Regression test for a real bug seen live: with no `extension_dir`
/// configured (true for the plain console binary), `setup.extensionPath` is
/// `null`, and `renderExtensionFolderPath` correctly set `hidden = true` on
/// `#extension-folder-path-row` -- but `.home-path-row` in styles.css also
/// declares `display: flex`, an author-origin rule of equal specificity to
/// the browser's default `[hidden] { display: none }`, which wins the
/// cascade over the user-agent default and kept the row (a bare "..." code
/// chip plus a Copy button that copies nothing) visible regardless of the
/// `hidden` attribute. This checks both halves: the JS computes `hidden`
/// correctly for the null case, and the CSS actually has an author-origin
/// rule that makes `hidden` win for that element.
#[test]
fn manual_extension_path_row_is_actually_hidden_when_path_is_unknown() {
    let css = fs::read_to_string("public/styles.css").unwrap();
    assert!(
        css.contains(".home-path-row[hidden] { display: none; }"),
        "styles.css must force `.home-path-row` to actually disappear when hidden \
         -- otherwise its own `display: flex` rule (equal specificity, author origin) \
         always wins over the `hidden` attribute, no matter what public/app.js sets it to"
    );

    let script = r#"
      import fs from "node:fs";

      const source = fs.readFileSync("public/app.js", "utf8");
      const start = source.indexOf("function renderExtensionFolderPath(state) {");
      const end = source.indexOf("\n\n// Self-contained copy/paste block", start);
      if (start < 0 || end < 0) throw new Error("missing renderExtensionFolderPath in public/app.js");
      const fnSource = source.slice(start, end);

      function makeElement() {
        return { hidden: false, textContent: "" };
      }

      const makeFn = new Function("ui", `${fnSource}\n return renderExtensionFolderPath;`);

      function run(extensionPath) {
        const ui = {
          "extension-folder-path-row": makeElement(),
          "extension-folder-unknown": makeElement(),
          "extension-folder-path": makeElement(),
        };
        makeFn(ui)({ setup: { extensionPath } });
        return ui;
      }

      for (const missing of [null, undefined, ""]) {
        const ui = run(missing);
        if (!ui["extension-folder-path-row"].hidden) {
          throw new Error(`extensionPath ${JSON.stringify(missing)} must hide the path row`);
        }
        if (ui["extension-folder-unknown"].hidden) {
          throw new Error(`extensionPath ${JSON.stringify(missing)} must show the fallback sentence`);
        }
      }

      const known = run("/opt/local-browser-bridge/extension");
      if (known["extension-folder-path-row"].hidden) {
        throw new Error("a real extensionPath must show the path row");
      }
      if (!known["extension-folder-unknown"].hidden) {
        throw new Error("a real extensionPath must hide the fallback sentence");
      }
      if (known["extension-folder-path"].textContent !== "/opt/local-browser-bridge/extension") {
        throw new Error(`path text was not rendered: ${known["extension-folder-path"].textContent}`);
      }
    "#;

    let output = match Command::new("node")
        .args(["--input-type=module", "-e", script])
        .output()
    {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => panic!("failed to run extension-folder-path harness: {error}"),
    };
    assert!(
        output.status.success(),
        "extension-folder-path harness failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

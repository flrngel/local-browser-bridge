use std::collections::BTreeSet;
use std::fs;

use local_browser_bridge::server::ACTION_METHODS;
use serde_json::Value;

fn source(path: &str) -> String {
    fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

fn keys(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect()
}

#[test]
fn candidate_binder_is_exact_external_and_immutable() {
    let script = source("scripts/browser-evidence-candidate.ps1");
    for required in [
        "ValidatePattern('^(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)\\.(0|[1-9][0-9]*)$')",
        "ValidatePattern('^[0-9a-f]{40}$')",
        "ChecksumManifestSha256",
        "externallySuppliedSha256",
        "Checksum manifest must use the canonical SHA256SUMS.txt filename.",
        "Checksum manifest must contain exactly four canonical entries.",
        "Checksum manifest entry order or spelling is not canonical.",
        "Checksum manifest must be canonical LF-terminated ASCII.",
        "Repository HEAD does not equal FINAL_SHA.",
        "Repository must be clean, including untracked files.",
        "Repository must be an exact clean checkout without ignored files.",
        "Server executable does not match the canonical checksum manifest.",
        "Extension ZIP does not match the canonical checksum manifest.",
        "Extension ZIP inventory order or spelling is not canonical.",
        "Extension ZIP timestamps are not deterministic.",
        "checkout and extension ZIP",
        "checkout and extracted extension",
        "Get-ExtractedPayload $ExtractedPath",
        "Assert-PayloadEqual $checkoutPayload $extractedPayload",
        "$manifest.minimum_chrome_version -cne \"140\"",
        "$manifest.description -cne $script:ExtensionDescription",
        "Assert-ExactStringArray @($manifest.permissions) $script:ExtensionPermissions",
        "Assert-ExactStringArray @($manifest.host_permissions) $script:ExtensionHostPermissions",
        "Assert-ExactKeys $manifest.background @(",
        "Extension manifest must contain exactly two canonical content-script stages.",
        "Assert-ExactStringArray @($contentScripts[0].js) @(\"stop-guard.js\")",
        "$contentScripts[0].run_at -cne \"document_start\"",
        "Assert-ExactStringArray @($contentScripts[1].js) @(\"dom-core.js\", \"content.js\")",
        "$contentScripts[1].run_at -cne \"document_idle\"",
        "Assert-ExactKeys $manifest.action @(",
        "Assert-ExactKeys $manifest.content_security_policy @(",
        "$manifestMutations = @(",
        "Label = \"early Stop guard\"",
        "Label = \"content security policy\"",
        "Label = \"unexpected nested field\"",
        "Candidate binding accepted noncanonical manifest $($mutation.Label).",
        "Candidate payload or identity changed after browser acceptance.",
        "Candidate binding accepted a noncanonical checksum manifest.",
        "function New-RunNonce",
        "runNonce = New-RunNonce",
        "Candidate binding record run nonce is invalid.",
        "candidateBinding = Get-CandidateBindingDomain",
        "Candidate postflight domain does not match its candidate and preflight digest.",
    ] {
        assert!(
            script.contains(required),
            "candidate binder is missing {required}"
        );
    }

    for name in [
        "local-browser-bridge-v$ExpectedVersion-windows-x86_64.exe",
        "local-computer-helper-v$ExpectedVersion-windows-x86_64.exe",
        "local-browser-bridge-v$ExpectedVersion-macos-universal.tar.gz",
        "local-browser-bridge-extension-v$ExpectedVersion.zip",
    ] {
        assert!(
            script.contains(name),
            "canonical checksum name is missing: {name}"
        );
    }

    let expected_inventory = [
        "background.js",
        "content.js",
        "dom-core.js",
        "frame-agent.js",
        "lib.js",
        "manifest.json",
        "popup.css",
        "popup.html",
        "popup.js",
        "stop-guard.js",
        "LICENSE",
    ];
    let inventory_block = script
        .split("$script:ExtensionFiles = @(")
        .nth(1)
        .unwrap()
        .split(')')
        .next()
        .unwrap();
    for name in expected_inventory {
        assert!(inventory_block.contains(&format!("\"{name}\"")));
    }
    assert_eq!(inventory_block.matches('"').count(), 22);

    for forbidden in [
        "--remote-debugging-port",
        "--user-data-dir",
        "--load-extension",
        "Extensions.loadUnpacked",
        "node ",
        "npm ",
    ] {
        assert!(
            !script.contains(forbidden),
            "candidate binder must not control Chrome or need Node: {forbidden}"
        );
    }
}

#[test]
fn operator_template_reserves_api_coverage_for_the_machine_record() {
    let template: Value = serde_json::from_str(&source(
        "evidence/v0.12.2/browser/operator-results.template.json",
    ))
    .unwrap();
    assert_eq!(template["schemaVersion"], 1);
    assert_eq!(
        template["evidenceType"],
        "stock-user-chrome-operator-observations"
    );

    assert!(!template.as_object().unwrap().contains_key("apiCoverage"));
    assert!(
        !template["extension"]
            .as_object()
            .unwrap()
            .contains_key("id")
    );
    assert_eq!(
        keys(&template),
        BTreeSet::from([
            "cleanup",
            "candidateBinding",
            "environment",
            "evidenceType",
            "extension",
            "handback",
            "schemaVersion",
        ])
    );
    assert_eq!(
        keys(&template["candidateBinding"]),
        BTreeSet::from([
            "checksumManifestSha256",
            "extensionZipSha256",
            "extractedPayloadSha256",
            "finalSha",
            "preflightRecordSha256",
            "runNonce",
            "serverSha256",
        ])
    );
    for value in template["candidateBinding"].as_object().unwrap().values() {
        assert_eq!(value, "REPLACE_WITH_INITIALIZER");
    }
    assert_eq!(
        template["handback"]["stop"]["statusPollMethod"],
        "browser.control.status"
    );
    assert_eq!(
        template["handback"]["cancel"]["statusPollMethod"],
        "browser.control.status"
    );
    assert_eq!(
        template["handback"]["stop"]["reducedStatus"]["reason"],
        "released_by_user"
    );
    assert_eq!(
        template["handback"]["cancel"]["reducedStatus"]["reason"],
        "canceled_by_user"
    );
    assert_eq!(template["extension"]["cardCount"], 0);
    assert_eq!(template["extension"]["idPatternValid"], false);
    assert_eq!(
        template["extension"]["loadedDirectoryByteMatchesCandidateZip"],
        false
    );
    assert_eq!(template["extension"]["permissionsUiReviewed"], false);
    assert_eq!(template["extension"]["hostAccessUiReviewed"], false);
    assert!(
        !template["extension"]
            .as_object()
            .unwrap()
            .contains_key("permissions")
    );
    assert!(
        !template["extension"]
            .as_object()
            .unwrap()
            .contains_key("hostPermissions")
    );
    assert_eq!(template["environment"]["stockUserChrome"], false);
    assert_eq!(template["environment"]["manualChromeExtensionsLoad"], false);
    assert_eq!(
        template["environment"]["dedicatedWindowBoundToOwnedTarget"],
        false
    );
    assert_eq!(template["cleanup"]["developerModeRestored"], false);
    assert_eq!(
        template["cleanup"]["savedTokenClear"]["trustedPopupClick"],
        false
    );
    assert_eq!(
        template["cleanup"]["savedTokenClear"]["popupStateVerifiedAfterClear"],
        false
    );
    assert_eq!(
        template["cleanup"]["savedTokenClear"]["tokenConfigured"],
        true
    );
    assert_eq!(
        template["cleanup"]["savedTokenClear"]["clearButtonDisabled"],
        false
    );
}

#[test]
fn finalizer_is_allowlisted_and_requires_handback_polling_cleanup_and_review() {
    let script = source("scripts/write-browser-evidence-record.ps1");
    for required in [
        "Assert-ApiMatrixRecord",
        "Assert-CandidatePreflight",
        "Candidate postflight does not bind the supplied preflight record.",
        "Candidate preflight and postflight bindings differ.",
        "$preflightSha256 = Get-Sha256 $preflightPath",
        "Get-CandidateBindingDomain",
        "Assert-CandidateBindingDomain",
        "Evidence finalizer accepted an API matrix from another candidate run.",
        "Evidence finalizer accepted operator results from another candidate run.",
        "Evidence finalizer accepted a screenshot from another candidate run.",
        "Candidate-bound operator checklist was initialized.",
        "candidate manifest permissions",
        "manifestPermissions = @($candidate.extension.permissions)",
        "API matrix must contain exactly 25 method results.",
        "Assert-ExactPropertyOrder $Record.assertions $expectedAssertions",
        "$item.stage -cne $script:MethodStages[$script:Methods[$index]]",
        "commandInvoked",
        "resultVerified",
        "postconditionVerified",
        "machine-command-result-postcondition",
        "$script:MethodScreenshots[$item.name]",
        "Evidence finalizer accepted a boolean as an integer.",
        "Evidence finalizer accepted an undersized screenshot.",
        "Evidence finalizer accepted a matrix method without a verified postcondition.",
        "Evidence finalizer accepted a noncanonical matrix screenshot mapping.",
        "apiMatrixRecordSha256",
        "extensionVersionMatched",
        "browserFloorMet",
        "topLayerControlUiIntegrity",
        "statusPolledAfterTrigger",
        "must poll browser.control.status after Resume",
        "HUMAN_CONTROL_PAUSED",
        "Assert-IntegerRange $Refusal.httpStatus 423 423",
        "released_by_user",
        "canceled_by_user",
        "trustedPopupClick",
        "saved token cleanup proof",
        "saved token cleanup tokenConfigured",
        "loadedDirectoryByteMatchesCandidateZip",
        "Exactly eleven screenshot sidecars are required.",
        "manualVisualReviewConfirmed",
        "automaticPixelRedactionPerformed",
        "unknownPixelSafetyClaimed",
        "Get-PngFacts",
        "Sanitized screenshot dimensions do not match their sidecar.",
        "rawApiResponsesRetained = $false",
        "chromeMcpTranscriptRetained = $false",
        "filesystemLocationsRetained = $false",
        "browserAccountOrProfileStateRetained = $false",
        "Final evidence contains an exact denylisted value.",
    ] {
        assert!(script.contains(required), "finalizer is missing {required}");
    }
    for forbidden in ["Invoke-RestMethod", "Invoke-WebRequest", "Start-Transcript"] {
        assert!(
            !script.contains(forbidden),
            "finalizer must validate reduced records, not collect raw responses: {forbidden}"
        );
    }
    assert!(!script.contains("candidateVersionMatched"));
}

#[test]
fn screenshot_tool_strips_metadata_but_never_claims_unknown_pixel_redaction() {
    let script = source("scripts/sanitize-browser-evidence-screenshot.ps1");
    let finalizer = source("scripts/write-browser-evidence-record.ps1");
    let readme = source("evidence/v0.12.2/browser/README.md");
    for required in [
        "ManualVisualReviewConfirmed is mandatory",
        "metadataStrippedByDecodeAndReencode = $true",
        "forbiddenMetadataChunksPresent = $false",
        "automaticPixelRedactionPerformed = $false",
        "unknownPixelSafetyClaimed = $false",
        "OCR is supplemental",
        "tesseract.exe, tesseract",
        "Get-PngChunkTypes",
        "Get-Sha256 $temporaryImage",
        "[switch]$AllowCanonicalBindingHex",
        "Test-ForbiddenText $serialized $denyValues -AllowCanonicalBindingHex",
        "Get-CandidateBindingFromPreflight",
        "candidateBinding = $candidateBinding",
        "Screenshot sanitizer secret/hash distinction self-test failed.",
        "CropX, CropY, CropWidth, and CropHeight must be supplied together.",
        "OutputImage is too small to prove the named visible UI state.",
        "$script:MinOutputWidth = 120",
        "$script:MinOutputHeight = 32",
        "(?i)(?<![a-p])[a-p]{32}(?![a-p])",
    ] {
        assert!(
            script.contains(required),
            "screenshot tool is missing {required}"
        );
    }
    for chunk in ["tEXt", "zTXt", "iTXt", "eXIf", "iCCP", "tIME"] {
        assert!(script.contains(&format!("\"{chunk}\"")));
    }
    for purpose in [
        "extensions-card",
        "extension-details",
        "popup-connected",
        "native-debugger-warning",
        "page-control-pill",
        "action-result",
        "stop-after",
        "stop-paused-popup",
        "cancel-after",
        "cancel-paused-popup",
        "resume-active",
    ] {
        assert!(script.contains(&format!("\"{purpose}\"")));
        assert!(finalizer.contains(&format!("\"{purpose}\"")));
        assert!(readme.contains(&format!("`{purpose}`")));
    }
    assert!(!script.contains("automaticPixelRedactionPerformed = $true"));
    assert!(!script.contains("unknownPixelSafetyClaimed = $true"));
    assert!(!script.contains("popup-release-after"));
    assert!(!script.contains("popup-release-paused-popup"));
}

#[test]
fn operator_protocol_is_stock_chrome_manual_and_fail_closed() {
    let readme = source("evidence/v0.12.2/browser/README.md");
    for required in [
        "already installed, ordinary Google Chrome",
        "designated interactive Windows acceptance host",
        "chrome://extensions",
        "Google Chrome MCP",
        "exactly one card",
        "Do not click **Load unpacked** again.",
        "More than one matching card is a failed preflight",
        "browser.control.status",
        "Do **not** poll raw",
        "HTTP 423",
        "HUMAN_CONTROL_PAUSED",
        "released_by_user",
        "canceled_by_user",
        "Resume remote control",
        "test-windows-browser-api.ps1",
        "-PassThruOwnedTarget",
        "$OwnedTarget = $MatrixHandoffJson | ConvertFrom-Json",
        "never rediscover a target by public URL, title",
        "dedicatedWindowBoundToOwnedTarget:true",
        "policy-checked",
        "tabs.new { url }",
        "topLayerControlUiIntegrity",
        "hostile generic popover declarations",
        "opaque `::before`/`::after`",
        "computed opacity, filter, mask, and transform",
        "transparent backdrop with no image",
        "filter, mask, clipping, transform, or pointer interaction",
        "real page content plus",
        "not proof of physical display or compositor output",
        "a page can race after any sampled instant",
        "browser-owned debugger notice remains the trusted handback surface",
        "opaque, full-viewport,",
        "same-process `srcdoc` iframe",
        "33 additional same-origin `srcdoc` frames",
        "two complete bridge-owned evaluate/automatic-observe",
        "exact direct child of `document.documentElement`",
        "DOM.getTopLayerElements",
        "DOM.getNodeForLocation(ignorePointerEventsNone:true)",
        "controlled root document",
        "five stable pill/Stop points",
        "tail of",
        "skipping child-document nodes",
        "narrow opaque",
        "avoids the five stable hit-point rows",
        "experimental CDP",
        "closed-shadow marker",
        "re-shows itself every animation frame",
        "copies the public host ID",
        "genuine ID unchanged",
        "`display:contents` light-DOM, open-shadow, or closed-shadow",
        "five seconds",
        "former mutation-depth window",
        "HTTP 409 `STALE_SNAPSHOT`",
        "recovery hint",
        "long View Transition",
        "`aria-hidden:true`",
        "2.5 second browser/message scheduling margin",
        "browser.control.status`—which does not call",
        "four real screenshot observations",
        "stopImmediatePropagation()",
        "control_ui_hidden",
        "requiresExplicitStart:true",
        "without navigating or reloading",
        "installed at `document_start`",
        "control must fail closed",
        "Never reload an unrelated user tab",
        "only after the candidate",
        "machine-verified unpacked payload",
        "exact `$ExtensionDirectory` that",
        "Page top-layer acceptance fixture",
        "The passing reduced record is the coverage authority.",
        "Every row below is machine-proven",
        "commandInvoked",
        "resultVerified",
        "postconditionVerified",
        "machine-command-result-postcondition",
        "literal `N/A`",
        "in-page Stop evidence below is a distinct human handback trigger",
        "-ApiMatrixRecord",
        "idPatternValid",
        "loadedDirectoryByteMatchesCandidateZip",
        "developerModeRestored",
        "Clear saved token",
        "clearSavedToken",
        "tokenConfigured:false",
        "savedTokenClear",
        "clearButtonDisabled",
        "the active popup status; it contains the in-memory numeric tab ID",
        "no popup or tab ID is included",
        "-PreflightRecord",
        "must exclude the actual extension ID line",
        "extension-details",
        "binder supplies the exact raw arrays",
        "all eleven reviewed image hashes",
        "same exact `candidateBinding` object",
        "cannot be mixed in",
        "does not claim continuous filesystem monitoring",
        "cannot exclude a",
        "temporarily swapping bytes",
        "no script claims to redact unknown pixels automatically",
        "Exclude **Current site**, the **Allowed sites** list",
        "The sanitizer cannot recognize",
        "every hostname or opaque numeric identifier",
        "Do not retain API response bodies",
        "Stop only `$ServerProcess`",
        "release the Google Chrome",
    ] {
        assert!(
            readme.contains(required),
            "operator protocol is missing {required}"
        );
    }
    for method in ACTION_METHODS {
        assert!(
            readme.contains(&format!("`{method}`")),
            "operator matrix is missing {method}"
        );
    }
    let mapped_methods: Vec<(usize, &str)> = readme
        .lines()
        .filter_map(|line| {
            let cells: Vec<&str> = line.split('|').map(str::trim).collect();
            let index = cells.get(1)?.parse::<usize>().ok()?;
            let method = cells.get(2)?.strip_prefix('`')?.strip_suffix('`')?;
            Some((index, method))
        })
        .collect();
    assert_eq!(mapped_methods.len(), ACTION_METHODS.len());
    for (offset, ((index, method), expected)) in
        mapped_methods.iter().zip(ACTION_METHODS.iter()).enumerate()
    {
        assert_eq!(*index, offset + 1);
        assert_eq!(method, expected);
    }
    let upgrade_disable = readme
        .find("For an upgrade, release control and disable")
        .expect("upgrade must disable the existing identity first");
    let owned_folder = readme
        .find("already known, operator-owned stable unpacked folder")
        .expect("upgrade must require an operator-owned stable folder");
    let extraction = readme
        .find("Expand-Archive -LiteralPath")
        .expect("protocol must include the candidate extraction step");
    assert!(upgrade_disable < owned_folder && owned_folder < extraction);
    assert_eq!(readme.matches("Expand-Archive -LiteralPath").count(), 1);
    assert!(readme.contains("never infer ownership from a folder name alone"));
    assert!(!readme.contains("FLRngel"));
}

#[test]
fn token_cleanup_proof_is_bound_to_the_trusted_popup_and_reduced_state() {
    let background = source("extension/background.js");
    let popup = source("extension/popup.html");
    let popup_script = source("extension/popup.js");
    let finalizer = source("scripts/write-browser-evidence-record.ps1");
    let readme = source("evidence/v0.12.2/browser/README.md");

    for required in [
        "id=\"clear-token\"",
        "Clear saved token",
        "Clearing the saved token disconnects the extension",
    ] {
        assert!(popup.contains(required), "popup is missing {required}");
    }
    for required in [
        "update(\"clearSavedToken\")",
        "ui.clearToken.disabled = !next.tokenConfigured",
    ] {
        assert!(
            popup_script.contains(required),
            "popup script is missing {required}"
        );
    }
    for required in [
        "case \"clearSavedToken\"",
        "assertTrustedPopupSender(sender)",
        "removeSecuritySettings([\"token\"], \"saved_token_cleared\")",
        "if (state.tokenConfigured || state.connectionStatus !== \"not-configured\")",
    ] {
        assert!(
            background.contains(required),
            "background is missing {required}"
        );
    }
    for required in [
        "savedTokenClear",
        "trustedPopupClick",
        "popupStateVerifiedAfterClear",
        "tokenConfigured",
        "clearButtonDisabled",
    ] {
        assert!(
            finalizer.contains(required),
            "finalizer is missing {required}"
        );
        assert!(
            readme.contains(&format!("`{required}")),
            "protocol is missing {required}"
        );
    }
    assert!(readme.contains("Do not inspect `chrome.storage`"));
    assert!(readme.contains("never the token or raw extension storage"));
}

#[test]
fn new_browser_evidence_files_are_english_only() {
    for path in [
        "scripts/browser-evidence-candidate.ps1",
        "scripts/sanitize-browser-evidence-screenshot.ps1",
        "scripts/test-windows-browser-api.ps1",
        "scripts/write-browser-evidence-record.ps1",
        "evidence/v0.12.2/browser/README.md",
        "evidence/v0.12.2/browser/operator-results.template.json",
    ] {
        let text = source(path);
        assert!(
            !text
                .chars()
                .any(|ch| ('\u{ac00}'..='\u{d7af}').contains(&ch)),
            "distributed evidence tooling must remain English-only: {path}"
        );
    }
}

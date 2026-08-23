use std::collections::BTreeSet;
use std::fs;

use local_browser_bridge::server::ACTION_METHODS;
use serde_json::Value;
use sha2::{Digest, Sha256};

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
        "function Get-RootPreservingFullDirectoryPath",
        "[IO.Path]::GetPathRoot($resolved)",
        "$pathComparer.Equals($resolved, $fileSystemRoot)",
        "function Test-IsTruePathNotFoundException",
        "$current -is [IO.FileNotFoundException]",
        "$current -is [IO.DirectoryNotFoundException]",
        "$current = $current.InnerException",
        "function Test-ExactPathAbsent",
        "[void][IO.File]::GetAttributes($Path)",
        "function Resolve-ExactSelfTestCleanupRoot",
        "^lbb-browser-candidate-[0-9a-f]{32}$",
        "function Remove-ExactSelfTestTreeOnce",
        "Self-test cleanup refused a reparse point in its temporary fixture.",
        "[IO.FileAttributes]::ReadOnly",
        "$script:SelfTestCleanupRetryMilliseconds",
        "cleanup-read-only.probe",
        "function Remove-ExactSelfTestDirectory",
        "Remove-ExactSelfTestDirectory $root",
        "Self-test cleanup root-preserving path normalization failed.",
        "Self-test cleanup missing-path probe failed.",
        "Self-test cleanup existing-path probe failed.",
        "$fixtureOwned = $true",
        "function Initialize-TrustedGitExecutable",
        "TrustedGitExecutable must be an absolute path for v0.12.8.",
        "& $script:GitExecutable --no-replace-objects --no-lazy-fetch `",
        "-c core.longpaths=true -c core.fsmonitor=false -c core.hooksPath=$script:EmptyHooksDirectory `",
        "Preserve the v0.12.2 contract",
        "Candidate binding lost its version-scoped hardened and legacy Git dispatch.",
    ] {
        assert!(
            script.contains(required),
            "candidate binder is missing {required}"
        );
    }

    assert!(
        !script.lines().any(|line| {
            line.trim_start().starts_with("& git ")
                || line.contains("= & git ")
                || line.contains("= (& git ")
        }),
        "candidate binder must invoke only its resolved Git executable"
    );

    assert!(
        !script.contains("[IO.Directory]::Delete($root, $true)"),
        "self-test cleanup must not recursively delete an unvalidated path"
    );
    let cleanup_functions = script
        .split("function Get-PathStringComparer")
        .nth(1)
        .unwrap()
        .split("function Invoke-GitText")
        .next()
        .unwrap();
    assert!(
        !cleanup_functions.contains("[IO.Directory]::Exists")
            && !cleanup_functions.contains("[IO.File]::Exists"),
        "cleanup boundaries must use the exception-bearing attribute probe"
    );
    let remove_once = cleanup_functions
        .split("function Remove-ExactSelfTestTreeOnce")
        .nth(1)
        .unwrap()
        .split("function Remove-ExactSelfTestDirectory")
        .next()
        .unwrap();
    assert!(
        remove_once.find("Test-ExactPathAbsent $RootPath").unwrap()
            < remove_once.find("$pending.Push($RootPath)").unwrap()
    );
    let remove_with_retries = cleanup_functions
        .split("function Remove-ExactSelfTestDirectory")
        .nth(1)
        .unwrap();
    assert!(
        remove_with_retries
            .find("Remove-ExactSelfTestTreeOnce $rootPath")
            .unwrap()
            < remove_with_retries
                .find("Test-ExactPathAbsent $rootPath")
                .unwrap()
    );
    let self_test = script
        .split("function Invoke-SelfTest")
        .nth(1)
        .unwrap()
        .split("switch ($Mode)")
        .next()
        .unwrap();
    let root_normalization = self_test
        .find("Get-RootPreservingFullDirectoryPath $fileSystemRoot")
        .unwrap();
    let missing_probe = self_test
        .find("if (-not (Test-ExactPathAbsent $root))")
        .unwrap();
    let create_root = self_test
        .find("[IO.Directory]::CreateDirectory($root)")
        .unwrap();
    let existing_probe = self_test.find("if (Test-ExactPathAbsent $root)").unwrap();
    let read_only_probe = self_test.find("cleanup-read-only.probe").unwrap();
    let exact_cleanup = self_test
        .rfind("Remove-ExactSelfTestDirectory $root")
        .unwrap();
    let passed = self_test
        .find("Browser candidate binding self-test passed.")
        .unwrap();
    assert!(root_normalization < missing_probe);
    assert!(missing_probe < create_root && create_root < existing_probe);
    assert!(read_only_probe < exact_cleanup && exact_cleanup < passed);

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
fn withdrawn_v0127_browser_protocol_is_byte_exact_and_unexecuted() {
    for (path, expected) in [
        (
            "evidence/v0.12.7/browser/README.md",
            "97b59051802d2c021cf3e6abe24219cc7638d1219da153237c74b89c12dd9a11",
        ),
        (
            "evidence/v0.12.7/browser/operator-results.template.json",
            "08bdee4bf03de02eebfbc4b30aefc42b6df0b200b7f67d5cf165c260b0ddc1d4",
        ),
        (
            "evidence/v0.12.7/browser/operator-results.schema.json",
            "a44d7ff6048e11cc8d6a62d69de6a308000e6511c4b83f57e2be03c3ce45ddf9",
        ),
        (
            "evidence/v0.12.7/browser/computer-helper-chain.schema.json",
            "675e94afdde9563348b1a3a8e14464cba520f236c1bfbfad23f9b7628d7d48cb",
        ),
    ] {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            expected,
            "withdrawn browser protocol changed: {path}"
        );
    }

    let entries = fs::read_dir("evidence/v0.12.7/browser")
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        entries,
        BTreeSet::from([
            "README.md".to_owned(),
            "computer-helper-chain.schema.json".to_owned(),
            "operator-results.schema.json".to_owned(),
            "operator-results.template.json".to_owned(),
        ])
    );

    let readme = source("evidence/v0.12.7/browser/README.md");
    assert!(readme.contains("before Chrome was started"));
    assert!(readme.contains("no `browser-acceptance.json`"));
    assert!(readme.contains("no v0.12.7 GitHub Release was created"));
}

#[test]
fn v0122_browser_protocol_is_byte_exact_while_v0128_uses_schema_two() {
    let readme_bytes = fs::read("evidence/v0.12.2/browser/README.md").unwrap();
    let template_bytes =
        fs::read("evidence/v0.12.2/browser/operator-results.template.json").unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(readme_bytes)),
        "07a2088a3bf86188f8274b9651183578fa64ceefcba44bace73bbabfe4b27072"
    );
    assert_eq!(
        format!("{:x}", Sha256::digest(template_bytes)),
        "abc28e66f4a14426d9d3cca3370354300354bb07e3c973a4d965e0f3606b8ac4"
    );

    let template: Value = serde_json::from_str(&source(
        "evidence/v0.12.8/browser/operator-results.template.json",
    ))
    .unwrap();
    let schema: Value = serde_json::from_str(&source(
        "evidence/v0.12.8/browser/operator-results.schema.json",
    ))
    .unwrap();
    assert_eq!(template["schemaVersion"], 2);
    assert_eq!(template["extension"]["version"], "0.12.8");
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["schemaVersion"]["const"], 2);
    assert_eq!(
        keys(&template),
        BTreeSet::from([
            "actionSurfaces",
            "candidateBinding",
            "cleanup",
            "computerHelperChain",
            "consentCheckpoints",
            "environment",
            "evidenceType",
            "extension",
            "humanVisualReview",
            "initialState",
            "restoration",
            "retainedEvidence",
            "schemaVersion",
            "screenshotCaptures",
        ])
    );
    assert_eq!(template["screenshotCaptures"].as_array().unwrap().len(), 3);
    assert_eq!(
        template["computerHelperChain"]["screenshotEndpoint"],
        "/api/computer/screenshot"
    );
    assert_eq!(template["computerHelperChain"]["rawScreenshotCount"], 0);
    assert_eq!(
        template["actionSurfaces"]["bridgeApiMatrix"],
        "local-browser-bridge-api"
    );
    assert_eq!(
        template["actionSurfaces"]["debuggerOwnerDuringBridgeLease"],
        "local-browser-bridge-extension"
    );
    assert_eq!(
        template["actionSurfaces"]["competingDebuggerAttachmentAllowed"],
        true
    );
    assert_eq!(
        template["actionSurfaces"]["chromeMcpUsedDuringBridgeLease"],
        true
    );
    assert_eq!(template["initialState"]["candidateExtensionPresent"], true);
    assert_eq!(template["initialState"]["savedTokenConfigured"], true);
    assert_eq!(
        template["cleanup"]["savedTokenClear"]["confirmationDialogShown"],
        false
    );
    assert_eq!(
        template["cleanup"]["savedTokenClear"]["confirmationAcceptedByHuman"],
        false
    );
    assert_eq!(
        template["retainedEvidence"]["externalToolAndPlatformLogsScope"],
        "not-asserted"
    );
    for field in [
        "rawScreenshotScratchDeleted",
        "pendingReviewRecordsDeleted",
        "extractedExtensionInventoryVerifiedBeforeDeletion",
        "extractedExtensionDirectoryDeleted",
    ] {
        assert_eq!(template["cleanup"][field], false);
        assert_eq!(
            schema["properties"]["cleanup"]["properties"][field]["const"],
            true
        );
    }
    for capture in template["screenshotCaptures"].as_array().unwrap() {
        assert_ne!(capture["captureSurface"], "google-chrome-mcp-tab-viewport");
    }
}

#[test]
fn v0128_finalizer_enforces_surface_consent_restoration_and_retention_relations() {
    let script = source("scripts/write-browser-evidence-record.ps1");
    let readme = source("evidence/v0.12.8/browser/README.md");
    for required in [
        "$script:OperatorV2Version = \"0.12.8\"",
        "Assert-OperatorResultsV1",
        "Assert-OperatorResultsV2",
        "No stock-user-Chrome operator schema is registered",
        "v$templateVersion",
        "bridgeApiMatrix",
        "debuggerOwnerDuringBridgeLease",
        "The Local Browser Bridge extension must be the exclusive debugger owner during its lease.",
        "chromeMcpUsedDuringBridgeLease",
        "chromeMcpReleaseEvidenceClaimed",
        "Developer Mode change tracking",
        "Developer Mode change consent",
        "final value does not equal its captured initial value",
        "initial candidate extension presence",
        "final candidate-extension presence",
        "initial saved token configured",
        "final saved-token state",
        "extension-disposition consent",
        "confirmationDialogShown",
        "confirmationAcceptedByHuman",
        "v0.12.8 screenshot identity, helper surface, or visible-state criterion is invalid.",
        "automationPausedForHumanReview",
        "postSanitizationAttestationCreated",
        "Assert-RetainedEvidenceDirectoryV2",
        "exactly 11 unique inputs",
        "externalToolAndPlatformLogsScope = \"not-asserted\"",
        "rawScreenshotScratchDeleted",
        "pendingReviewRecordsDeleted",
        "extractedExtensionInventoryVerifiedBeforeDeletion",
        "extractedExtensionDirectoryDeleted",
        "Evidence finalizer accepted missing Developer Mode change tracking.",
        "Evidence finalizer accepted a Developer Mode change without action-time consent.",
        "Evidence finalizer accepted a Developer Mode change when the required test state already matched.",
        "Evidence finalizer accepted a Developer Mode restoration mismatch.",
        "Evidence finalizer accepted a Full Access restoration mismatch.",
        "Evidence finalizer accepted test-copy removal without ownership consent.",
        "Evidence finalizer accepted candidate installation without ownership consent.",
        "Evidence finalizer accepted an existing candidate extension identity.",
        "Evidence finalizer accepted an initially configured saved token.",
        "Evidence finalizer accepted a configured final saved token.",
        "Evidence finalizer accepted a retained test-owned extension identity.",
        "Evidence finalizer accepted a non-removal disposition for the test-owned identity.",
        "Evidence finalizer accepted a browser-chrome screenshot from Chrome MCP.",
        "Evidence finalizer accepted a screenshot surface inconsistent with the declared capture surface.",
        "Evidence finalizer accepted Chrome MCP use during an active bridge debugger lease.",
        "Evidence finalizer accepted a Chrome MCP release-evidence claim.",
        "Evidence finalizer accepted a competing debugger attachment during the bridge lease.",
        "Evidence finalizer accepted an incomplete clear-token confirmation.",
        "Evidence finalizer accepted an automated visual-review assertion.",
        "Evidence finalizer accepted review asserted before post-sanitization attestation.",
        "Evidence finalizer accepted retained raw screenshot scratch data.",
        "Evidence finalizer accepted retained pending-review records.",
        "Evidence finalizer accepted unverified extracted-extension cleanup.",
        "Evidence finalizer accepted a retained extracted-extension directory.",
        "Evidence finalizer complete v0.12.8 self-test failed.",
    ] {
        assert!(
            script.contains(required),
            "v0.12.8 finalizer is missing {required}"
        );
    }
    for required in [
        "Local Browser Bridge attaches `chrome.debugger`",
        "Chrome permits only one debugger client",
        "Chrome MCP cannot coexist",
        "Windows Computer Use application sharing",
        "The Local Browser Bridge API executes the browser method matrix.",
        "chromeMcpUsedDuringBridgeLease:false",
        "competingDebuggerAttachmentAllowed:false",
        "chromeMcpReleaseEvidenceClaimed:false",
        "chromeMcpReleasedOrNotUsed:true",
        "native **Load unpacked** picker",
        "Browser Bridge popup",
        "Consent cannot be collected as one blanket approval",
        "new-install-only",
        "card exists before **Load unpacked**",
        "initially unconfigured",
        "confirmationAcceptedByHuman:true",
        "Automation must then pause.",
        "Exact purpose-and-image-digest-bound human review receipt",
        "AttestReview refused a missing or mismatched per-image human receipt.",
        "-Mode AttestReview",
        "manualVisualReviewConfirmed:false",
        "acceptance-evidence-directory-only",
        "imported or cited as release evidence",
        "Repository HEAD does not equal FINAL_SHA.",
        "Repository HEAD must be detached.",
        "Repository checkout must be clean, including untracked files.",
        "Repository checkout must not contain ignored files.",
        "A repository evidence script does not byte-match FINAL_SHA.",
        "Export-ExactTrustedBlob",
        "Materialized trusted blob does not byte-match FINAL_SHA.",
        "GITHUB_",
        "Independent least-privilege GitHub acceptance token",
        "-NoLogo -NoProfile -File",
        "LBB_CLEAN_COORDINATOR_NONCE",
        "self-spawned 64-bit -NoProfile child",
        "$SecureGhToken.Dispose()",
        "$Info.EnvironmentVariables[\"GH_TOKEN\"] = $ChildToken",
        "--hostname github.com",
        "try {\n$Repository = New-ShortSourceDirectory",
        "$ShortSourceParent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\\')",
        "$ShortSourceParent.Length -gt 80",
        "function New-ShortSourceDirectory",
        "core.longpaths=true",
        "config --local core.longpaths true",
        "The system temporary directory must be an existing short ordinary source parent.",
        "Remove-ExactFlatOwnedDirectory $RawScreenshotDirectory",
        "Remove-ExactFlatOwnedDirectory $ExtensionDirectory",
        "Acceptance cleanup failed and passing output was invalidated",
        "-PassThruOwnedTarget",
        "-ComputerHelperRecord",
        "browser-computer-helper-chain.json",
        "browser-01-extension-loaded.png",
        "browser-02-api-action-result.png",
        "browser-03-computer-share-action.png",
        "computer.status` results before and after `Control+N",
        "-Mode Postflight",
        "rawScreenshotScratchDeleted:true",
        "pendingReviewRecordsDeleted:true",
        "extractedExtensionDirectoryDeleted:true",
    ] {
        assert!(
            readme.contains(required),
            "v0.12.8 protocol is missing {required}"
        );
    }
    let asset_hash = readme.find("$ObservedAssetSha =").unwrap();
    let attest = readme
        .find("$Info.Arguments = \"attestation verify")
        .unwrap();
    let head = readme.find("$ObservedHead =").unwrap();
    let detached = readme.find("$SymbolicHead =").unwrap();
    let clean = readme.find("$Dirty =").unwrap();
    let ignored = readme.find("$Ignored =").unwrap();
    let blob = readme.find("$ExpectedBlob =").unwrap();
    let materialized = readme
        .find("Export-ExactTrustedBlob $ExpectedBlob $Relative")
        .unwrap();
    let extraction = readme
        .find("[IO.Compression.ZipFile]::ExtractToDirectory")
        .unwrap();
    let first_repo_script = readme
        .find("& \"$Scripts\\browser-evidence-candidate.ps1\"")
        .unwrap();
    assert!(
        asset_hash < attest
            && attest < head
            && head < detached
            && detached < clean
            && clean < ignored
            && ignored < blob
            && blob < materialized
            && materialized < extraction
            && blob < extraction
            && extraction < first_repo_script,
        "all external trust gates and direct blob materialization must precede extraction and trusted-script execution"
    );
    assert_eq!(
        readme
            .matches("[IO.Compression.ZipFile]::ExtractToDirectory")
            .count(),
        1
    );
    assert!(!readme.contains("For an upgrade"));
    assert!(!readme.contains("install/upgrade"));
    assert!(!readme.contains("verify-release-assets.sh"));
    assert!(!readme.contains("& git -C $Repository"));
    assert!(!readme.contains("& gh "));
    let directory_creation = readme
        .find("$Repository = New-ShortSourceDirectory")
        .unwrap();
    let outer_try = readme[..directory_creation].rfind("try {").unwrap();
    let finalizer = readme
        .find("& \"$Scripts\\write-browser-evidence-record.ps1\" -Mode Finalize")
        .unwrap();
    let outer_finally = readme[finalizer..].find("finally {").unwrap() + finalizer;
    assert!(
        outer_try < directory_creation
            && directory_creation < finalizer
            && finalizer < outer_finally
    );

    let sanitize = readme.find("-Mode Sanitize").unwrap();
    let pause = readme.find("Automation must then pause.").unwrap();
    let review_receipt = readme
        .find("Exact purpose-and-image-digest-bound human review receipt")
        .unwrap();
    let attest_review = readme.find("-Mode AttestReview").unwrap();
    assert!(sanitize < pause && pause < review_receipt && review_receipt < attest_review);
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
        "$expectedScreenshotMap[$item.name]",
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
    let v0128_readme = source("evidence/v0.12.8/browser/README.md");
    for required in [
        "ManualVisualReviewConfirmed is mandatory for v0.12.2 compatibility.",
        "ManualVisualReviewConfirmed is valid only for AttestReview after a human has inspected the sanitized PNG.",
        "AttestReview requires action-time confirmation after a human inspected the sanitized PNG.",
        "stock-user-chrome-screenshot-review-pending",
        "manualVisualReviewConfirmed = $legacyOnePhase",
        "$pending.manualVisualReviewConfirmed -ne $false",
        "$script:PendingReviewStatement",
        "$script:CompletedReviewStatement",
        "$script:LegacyReviewStatement",
        "$script:CandidateVersionFromPreflight",
        "AttestReview is available only for the v0.12.8 two-phase screenshot protocol.",
        "The sanitized PNG changed after its pending review record was created.",
        "Screenshot sanitizer accepted review confirmation before creating the sanitized crop.",
        "Legacy v0.12.2 screenshot sanitization compatibility failed.",
        "metadataStrippedByDecodeAndReencode = $true",
        "forbiddenMetadataChunksPresent = $false",
        "automaticPixelRedactionPerformed = $false",
        "unknownPixelSafetyClaimed = $false",
        "OCR is supplemental",
        "tesseract.exe, tesseract",
        "Get-PngChunkTypes",
        "Get-Sha256 $temporaryImage",
        "$Object -is [Collections.IDictionary]",
        "Screenshot sanitizer exact ordered dictionary key self-test failed.",
        "Screenshot sanitizer self-test raw fixture does not satisfy the production byte floor.",
        "[switch]$AllowCanonicalBindingHex",
        "[switch]$AllowCanonicalEvidenceType",
        "Test-ForbiddenText $serialized $denyValues -AllowCanonicalBindingHex -AllowCanonicalEvidenceType",
        "$canonicalPendingLookalike",
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
    let sanitize = v0128_readme.find("-Mode Sanitize").unwrap();
    let human_pause = v0128_readme.find("Automation must then pause.").unwrap();
    let review_receipt = v0128_readme
        .find("Exact purpose-and-image-digest-bound human review receipt")
        .unwrap();
    let attest = v0128_readme.find("-Mode AttestReview").unwrap();
    assert!(sanitize < human_pause && human_pause < review_receipt && review_receipt < attest);
    assert!(readme.contains("-Mode Sanitize"));
    assert!(readme.contains("-ManualVisualReviewConfirmed"));
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
    let v0128_readme = source("evidence/v0.12.8/browser/README.md");

    for required in [
        "id=\"clear-token\"",
        "Clear saved token",
        "Clearing the saved token disconnects the extension",
    ] {
        assert!(popup.contains(required), "popup is missing {required}");
    }
    for required in [
        "confirm(\"Clear the saved extension token and disconnect now?",
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
    assert!(v0128_readme.contains("wait for the popup's native `confirm()` dialog"));
    assert!(v0128_readme.contains("confirmationAcceptedByHuman:true"));
}

#[test]
fn v0128_computer_helper_chain_is_live_exact_window_and_three_capture_bound() {
    let recorder = source("scripts/record-computer-helper-chain.ps1");
    let finalizer = source("scripts/write-browser-evidence-record.ps1");
    let schema: Value = serde_json::from_str(&source(
        "evidence/v0.12.8/browser/computer-helper-chain.schema.json",
    ))
    .unwrap();

    for required in [
        "$script:Source = \"local-browser-bridge-computer-helper-via-loopback-api\"",
        "Invoke-LoopbackJson \"/api/v1/command\"",
        "computer.share.start",
        "computer.share.stop",
        "[void](Bind-NewDedicatedChromeWindow)",
        "Control+N did not produce exactly one new stock-Chrome window",
        "Get-ExactChromeWindow $script:DedicatedWindowId $script:DedicatedWindowPid",
        "Test-ExactObservationIdentity",
        "Test-ExactSharedFrame",
        "Get-FreshSharedFrame $Context \"the $Purpose screenshot\"",
        "Never accept response-state observation as post-action authority",
        "accepted an observation from a reused HWND/wrong PID",
        "accepted an observation from a non-Chrome application",
        "Set-MutationDisposition $mutation \"DedicatedWindow\" \"outcome_unknown\"",
        "not_attempted",
        "verified_applied",
        "outcome_unknown",
        "Read-RollbackCandidateCardState",
        "Select Folder button in Chrome's native picker",
        "the exact existing chrome://extensions tab",
        "Stop-Process -Id $helperProcess.Id -Force",
        "UI rollback was unavailable because the exact helper/server transport was not alive",
    ] {
        assert!(
            recorder.contains(required),
            "live helper recorder is missing {required}"
        );
    }
    let screenshot_recorder = recorder
        .split("function Save-RecordedScreenshot")
        .nth(1)
        .unwrap()
        .split("function Invoke-BrowserCommand")
        .next()
        .unwrap();
    assert!(screenshot_recorder.contains("Get-FreshSharedFrame"));
    assert!(!screenshot_recorder.contains("Get-FreshObservation"));
    for forbidden in [
        "[string]$JournalRecord",
        "MayBeChanged",
        "MayExist",
        "computer.helper.shutdown",
    ] {
        assert!(
            !recorder.contains(forbidden),
            "live helper recorder retains self-asserted or unsafe state: {forbidden}"
        );
    }

    assert_eq!(schema["properties"]["screenshots"]["minItems"], 3);
    assert_eq!(schema["properties"]["screenshots"]["maxItems"], 3);
    assert_eq!(schema["properties"]["windowEpochs"]["minItems"], 9);
    assert_eq!(schema["properties"]["windowEpochs"]["maxItems"], 9);
    assert_eq!(schema["properties"]["actions"]["minItems"], 19);
    assert_eq!(schema["properties"]["actions"]["maxItems"], 19);
    assert_eq!(
        schema["properties"]["windowBinding"]["properties"]["dedicatedCreatedAsOnlyNewChromeWindow"]
            ["const"],
        true
    );
    assert_eq!(
        schema["$defs"]["epoch"]["properties"]["application"]["const"],
        "google-chrome"
    );

    for required in [
        "Dedicated Chrome epochs must bind the same exact native window and process.",
        "computer-helper sole-new dedicated target creation",
        "The native picker action must type the exact directory and invoke Select Folder.",
        "Cleanup must switch to chrome://extensions before the exact card removal and confirmation.",
        "The computer-helper record must bind exactly three raw exact-window screenshots.",
        "A sanitized screenshot sidecar does not bind its exact helper-captured raw PNG.",
        "Evidence finalizer accepted a different dedicated Chrome window in a later epoch.",
        "Evidence finalizer accepted a dedicated window without a sole-new status delta.",
    ] {
        assert!(
            finalizer.contains(required),
            "helper finalizer is missing {required}"
        );
    }
}

#[test]
fn new_browser_evidence_files_are_english_only() {
    for path in [
        "scripts/browser-evidence-candidate.ps1",
        "scripts/sanitize-browser-evidence-screenshot.ps1",
        "scripts/test-windows-browser-api.ps1",
        "scripts/write-browser-evidence-record.ps1",
        "scripts/record-computer-helper-chain.ps1",
        "evidence/v0.12.2/browser/README.md",
        "evidence/v0.12.2/browser/operator-results.template.json",
        "evidence/v0.12.7/browser/README.md",
        "evidence/v0.12.7/browser/operator-results.template.json",
        "evidence/v0.12.7/browser/operator-results.schema.json",
        "evidence/v0.12.7/browser/computer-helper-chain.schema.json",
        "evidence/v0.12.8/browser/README.md",
        "evidence/v0.12.8/browser/operator-results.template.json",
        "evidence/v0.12.8/browser/operator-results.schema.json",
        "evidence/v0.12.8/browser/computer-helper-chain.schema.json",
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

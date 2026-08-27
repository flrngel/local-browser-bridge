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
        "TrustedGitExecutable must be an absolute path for v0.12.41.",
        "& $script:GitExecutable --no-replace-objects --no-lazy-fetch `",
        "-c core.longpaths=true -c core.fsmonitor=false -c core.hooksPath=$script:EmptyHooksDirectory `",
        "Preserve the v0.12.2 contract",
        "Candidate binding lost its version-scoped hardened and legacy Git dispatch.",
        "Get-ValidatedReleaseCandidateBinding",
        "workflowRunAttempt",
        "attestationInvocationUri",
        "Candidate binding accepted a mismatched release workflow attempt.",
        "[IO.FileMode]::CreateNew",
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
        "$env:HOME =",
        "$env:USERPROFILE =",
        "$env:CODEX_HOME =",
        "SetEnvironmentVariable(\"HOME\"",
        "SetEnvironmentVariable(\"USERPROFILE\"",
        "SetEnvironmentVariable(\"CODEX_HOME\"",
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
            "811ec2a5025fa873cd818f45ac2fead8337f751f81a499eacdc2db33b86ed9c4",
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
fn withdrawn_v0128_browser_protocol_is_byte_exact_unexecuted_and_not_current() {
    for (path, expected) in [
        (
            "evidence/v0.12.8/browser/operator-results.template.json",
            "741759fbd365784297e8701dc9d3be63c7e74f5900896e8c92896f1a0f7cb676",
        ),
        (
            "evidence/v0.12.8/browser/operator-results.schema.json",
            "34b685e87b06c6cef7ac02a3984b9ce54fa5203f592f263cadd39dc66ff56119",
        ),
        (
            "evidence/v0.12.8/browser/computer-helper-chain.schema.json",
            "62cd290ee8d3b7eae1f4ed3c206490203ba0d6db97ec370fa486d996bb4b9c4a",
        ),
    ] {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            expected,
            "withdrawn v0.12.8 browser protocol changed: {path}"
        );
    }

    let entries = fs::read_dir("evidence/v0.12.8/browser")
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

    let readme = source("evidence/v0.12.8/browser/README.md");
    assert!(readme.contains("Withdrawn"));
    assert!(readme.contains("Chrome was never started"));
    assert!(readme.contains("no `browser-acceptance.json`"));
    assert!(readme.contains("no v0.12.8 GitHub Release exists"));

    let finalizer = source("scripts/write-browser-evidence-record.ps1");
    assert!(finalizer.contains("$script:OperatorV2Version = \"0.12.41\""));
    assert!(finalizer.contains(
        "No stock-user-Chrome operator schema is registered for candidate version $ExpectedVersion."
    ));
    assert!(!finalizer.contains("$script:OperatorV2Version = \"0.12.8\""));
    assert!(!finalizer.contains("evidence\", \"v0.12.8\", \"browser"));
}

#[test]
fn withdrawn_v0129_browser_protocol_is_byte_exact_unexecuted_and_not_current() {
    for (path, expected) in [
        (
            "evidence/v0.12.9/browser/README.md",
            "f7550b5660cf8e1980a5c2ed30406661a4aef450411423d124675cec929e1c51",
        ),
        (
            "evidence/v0.12.9/browser/operator-results.template.json",
            "86d9a77f8cc99a45f64ae5be4c0db746fbfafa957045f83522b472a9d8efbde7",
        ),
        (
            "evidence/v0.12.9/browser/operator-results.schema.json",
            "a7467f637e0501230f1c0d78fab80c623913d69c6a9dc4bf773eb12da14d453b",
        ),
        (
            "evidence/v0.12.9/browser/computer-helper-chain.schema.json",
            "4a6376f6f3b2ae19385e3f8e68cefc03becb6655f3c6f9f43db1804f9db9b1e1",
        ),
    ] {
        assert_eq!(
            format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
            expected,
            "withdrawn v0.12.9 browser protocol changed: {path}"
        );
    }

    let entries = fs::read_dir("evidence/v0.12.9/browser")
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

    let readme = source("evidence/v0.12.9/browser/README.md");
    assert!(readme.contains("No v0.12.9 stock-Chrome acceptance result exists yet."));
    assert!(readme.contains("It is protocol infrastructure, not passing"));

    let finalizer = source("scripts/write-browser-evidence-record.ps1");
    assert!(finalizer.contains("$script:OperatorV2Version = \"0.12.41\""));
    assert!(!finalizer.contains("$script:OperatorV2Version = \"0.12.9\""));
    assert!(!finalizer.contains("evidence\", \"v0.12.9\", \"browser"));
}

#[test]
fn v0122_browser_protocol_is_byte_exact_while_v01229_uses_schema_three() {
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

    let entries = fs::read_dir("evidence/v0.12.41/browser")
        .unwrap()
        .map(Result::unwrap)
        .map(|entry| {
            let file_type = entry.file_type().unwrap();
            assert!(
                file_type.is_file() && !file_type.is_symlink(),
                "current browser evidence scaffold entry must be an ordinary file: {}",
                entry.path().display()
            );
            entry.file_name().to_string_lossy().into_owned()
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(
        entries,
        BTreeSet::from([
            "README.md".to_owned(),
            "computer-helper-chain.schema.json".to_owned(),
            "external-surface-attestation.schema.json".to_owned(),
            "independent-visual-review.schema.json".to_owned(),
            "operator-results.schema.json".to_owned(),
            "operator-results.template.json".to_owned(),
            "scoped-action-approval.schema.json".to_owned(),
        ])
    );

    let template: Value = serde_json::from_str(&source(
        "evidence/v0.12.41/browser/operator-results.template.json",
    ))
    .unwrap();
    let schema: Value = serde_json::from_str(&source(
        "evidence/v0.12.41/browser/operator-results.schema.json",
    ))
    .unwrap();
    let helper_schema: Value = serde_json::from_str(&source(
        "evidence/v0.12.41/browser/computer-helper-chain.schema.json",
    ))
    .unwrap();
    let approval_schema: Value = serde_json::from_str(&source(
        "evidence/v0.12.41/browser/scoped-action-approval.schema.json",
    ))
    .unwrap();
    let review_schema: Value = serde_json::from_str(&source(
        "evidence/v0.12.41/browser/independent-visual-review.schema.json",
    ))
    .unwrap();
    let external_schema: Value = serde_json::from_str(&source(
        "evidence/v0.12.41/browser/external-surface-attestation.schema.json",
    ))
    .unwrap();
    assert_eq!(template["schemaVersion"], 3);
    assert_eq!(template["extension"]["version"], "0.12.41");
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert_eq!(schema["additionalProperties"], false);
    assert_eq!(schema["properties"]["schemaVersion"]["const"], 3);
    assert_eq!(helper_schema["properties"]["schemaVersion"]["const"], 2);
    assert_eq!(approval_schema["properties"]["schemaVersion"]["const"], 1);
    assert_eq!(review_schema["properties"]["schemaVersion"]["const"], 1);
    assert_eq!(external_schema["properties"]["schemaVersion"]["const"], 1);
    assert_eq!(
        external_schema["properties"]["orchestrationSurface"]["const"],
        "user-orchestrator-secured-ssh-exported-file-review"
    );
    assert_eq!(
        external_schema["properties"]["phase"]["enum"][0],
        "preflight"
    );
    assert_eq!(
        external_schema["properties"]["phase"]["enum"][1],
        "postflight"
    );
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
            "handback",
            "independentVisualReview",
            "initialState",
            "releaseCandidateBinding",
            "restoration",
            "retainedEvidence",
            "schemaVersion",
            "screenshotCaptures",
        ])
    );
    assert_eq!(template["screenshotCaptures"].as_array().unwrap().len(), 6);
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
        template["cleanup"]["savedTokenClear"]["confirmationAcceptedByExecutor"],
        false
    );
    assert!(template.get("humanVisualReview").is_none());
    assert!(
        template["cleanup"]["savedTokenClear"]
            .get("confirmationAcceptedByHuman")
            .is_none()
    );
    assert_eq!(template["retainedEvidence"]["inputFileCount"], 0);
    assert_eq!(template["retainedEvidence"]["finalFileCount"], 0);
    assert_eq!(
        schema["properties"]["retainedEvidence"]["properties"]["inputFileCount"]["const"],
        21
    );
    assert_eq!(
        schema["properties"]["retainedEvidence"]["properties"]["finalFileCount"]["const"],
        22
    );
    assert_eq!(
        approval_schema["properties"]["response"]["properties"]["deliveredBy"]["const"],
        "user-via-orchestrator"
    );
    assert_eq!(review_schema["properties"]["entries"]["minItems"], 6);
    assert_eq!(review_schema["properties"]["entries"]["maxItems"], 6);
    assert_eq!(
        helper_schema["properties"]["operatorExchange"]["properties"]["independentSessionBoundary"]
            ["const"],
        true
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
fn v01229_finalizer_enforces_surface_consent_restoration_and_retention_relations() {
    let script = source("scripts/write-browser-evidence-record.ps1");
    let coordinator = source("scripts/test-windows-stock-chrome.ps1");
    let readme = source("evidence/v0.12.41/browser/README.md");
    let operator_schema = source("evidence/v0.12.41/browser/operator-results.schema.json");

    assert!(operator_schema.contains(
        "\"orchestrationAndConsent\": { \"const\": \"user-orchestrator-secured-ssh-exported-file-review\" }"
    ));

    for required in [
        "$script:OperatorV2Version = \"0.12.41\"",
        "Assert-OperatorResultsV1",
        "Assert-OperatorResultsV2",
        "Assert-ScopedApprovalRecordV3",
        "Assert-IndependentReviewRecordV3",
        "Assert-ApprovalMatchesHelperV3",
        "Assert-ReviewMatchesHelperAndSidecarsV3",
        "Read-JsonWithSha256",
        "No stock-user-Chrome operator schema is registered",
        "bridgeApiMatrix",
        "debuggerOwnerDuringBridgeLease",
        "The Local Browser Bridge extension must be the exclusive debugger owner during its lease.",
        "chromeMcpUsedDuringBridgeLease",
        "chromeMcpReleaseEvidenceClaimed",
        "Developer Mode change",
        "final value does not equal its captured initial value",
        "initial candidate extension presence",
        "final candidate-extension presence",
        "initial saved token configured",
        "final saved-token state",
        "confirmationDialogShown",
        "confirmationAcceptedByExecutor",
        "independentVisualReview",
        "scopedActionTimeApproval",
        "user-via-orchestrator",
        "batched-action-time",
        "The scoped approval was not confirmed and unexpired at the first covered action.",
        "Scoped approval must use a user-facing orchestrator session distinct from both executor and reviewer.",
        "Assert-RetainedEvidenceDirectoryV2",
        "exactly 21 unique inputs",
        "releaseCandidateBinding",
        "operatorRecordSha256",
        "topologyProtocolBound",
        "exactHelperImageProcessesRemaining",
        "canonicalPortListenersRemaining",
        "automatedTextInspectionPerformed",
        "independentVisualReviewRequired",
        "independentVisualReviewCompleted",
        "externalToolAndPlatformLogsScope = \"not-asserted\"",
        "rawScreenshotScratchDeleted",
        "pendingReviewRecordsDeleted",
        "extractedExtensionInventoryVerifiedBeforeDeletion",
        "extractedExtensionDirectoryDeleted",
        "Evidence finalizer accepted a Developer Mode restoration mismatch.",
        "Evidence finalizer accepted a Full Access restoration mismatch.",
        "Evidence finalizer accepted a checkpoint bound to another approval.",
        "Evidence finalizer accepted an existing candidate extension identity.",
        "Evidence finalizer accepted an initially configured saved token.",
        "Evidence finalizer accepted a configured final saved token.",
        "Evidence finalizer accepted a retained test-owned extension identity.",
        "Evidence finalizer accepted a competing debugger attachment during the bridge lease.",
        "Evidence finalizer accepted clear-token confirmation bound to another approval.",
        "Evidence finalizer accepted an obsolete human visual-review object.",
        "Evidence finalizer accepted a same-session independent review.",
        "Evidence finalizer accepted a replayed scoped approval.",
        "Evidence finalizer accepted an uncertain independent review.",
        "Evidence finalizer accepted sensitive pixels in independent review.",
        "Evidence finalizer accepted reordered independent review entries.",
        "Evidence finalizer accepted a reordered approval scope.",
        "Evidence finalizer accepted retained raw screenshot scratch data.",
        "Evidence finalizer complete v0.12.41 self-test failed.",
    ] {
        assert!(
            script.contains(required),
            "v0.12.41 finalizer is missing {required}"
        );
    }

    for required in [
        "ordinary user's installed Google Chrome",
        "chrome://extensions",
        "native **Load unpacked** picker",
        "Do not relaunch Chrome with flags",
        "Local Browser Bridge owns `chrome.debugger`",
        "Chrome allows only one debugger client",
        "Chrome MCP must not attach",
        "The Local Browser Bridge API executes the browser-method matrix.",
        "One scoped action-time approval",
        "single candidate run",
        "user-via-orchestrator",
        "Independent executor and reviewer",
        "create-once request/response files",
        "response-<requestId>.json.new",
        "atomically renames it",
        "not cryptographic proof",
        "named-pipe channel",
        "visualJudgmentNotPixelSafetyProof: true",
        "test-windows-stock-chrome.ps1",
        "verify-windows-release-candidate.ps1",
        "exact 64-bit",
        "Windows PowerShell 5.1",
        "candidate-binding.json",
        "least-privilege GitHub token",
        "RelayGitHubToken",
        "NewSessionRef",
        "exercise visible **Stop**, native **Cancel**",
        "native debugger-use notice",
        "six sanitized PNGs",
        "exactly twenty-one ordinary files",
        "browser-acceptance.json",
        "single-parent release-evidence commit",
        "twenty-second file",
    ] {
        assert!(
            readme.contains(required),
            "v0.12.41 operator protocol is missing {required}"
        );
    }
    for forbidden in [
        "REPLACE_WITH_",
        "$ObservedAssetSha =",
        "[IO.Compression.ZipFile]::ExtractToDirectory",
        "Save every PowerShell block",
    ] {
        assert!(
            !readme.contains(forbidden),
            "operator documentation must delegate executable policy to the checked-in coordinator: {forbidden}"
        );
    }

    for required in [
        "#requires -Version 5.1",
        "[switch]$SelfTest",
        "Windows stock-Chrome coordinator self-test passed.",
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
        "[Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames",
        "The machine Windows SystemRoot is unavailable or invalid.",
        "System32\", \"WindowsPowerShell\", \"v1.0\", \"powershell.exe\"",
        "-NoLogo -NoProfile -File",
        "LBB_CLEAN_COORDINATOR_NONCE",
        "self-spawned 64-bit -NoProfile child",
        "LBB_COORDINATOR_WORKFLOW_RUN_ATTEMPT",
        "LBB_COORDINATOR_ARTIFACT_ID",
        "LBB_COORDINATOR_ARTIFACT_ZIP_SHA256",
        "Set-OwnerPrivateDirectoryAcl $Path",
        "$Security.SetAccessRuleProtection($true, $false)",
        "$Label ACL is not protected and private to the current user.",
        "Candidate binding does not match the exact workflow artifact and attempt.",
        "$EntryInvocation = $Predicate.runDetails.metadata.invocationId",
        "$CurrentAttemptCount -ne 1",
        "$Certificate.runInvocationURI",
        "$Certificate.runnerEnvironment -cne \"github-hosted\"",
        "Independent least-privilege GitHub acceptance token",
        "Read-GitHubAcceptanceToken",
        "NamedPipeClientStream",
        "LBB_COORDINATOR_GH_TOKEN_PIPE",
        "$Info.EnvironmentVariables[\"GH_TOKEN\"] = $ChildToken",
        "$SecureGhToken.Dispose()",
        "core.longpaths=true",
        "Repository HEAD does not equal FINAL_SHA.",
        "Repository HEAD must be detached.",
        "Repository checkout must be clean, including untracked files.",
        "Repository checkout must not contain ignored files.",
        "A repository evidence script does not byte-match FINAL_SHA.",
        "scripts/test-windows-stock-chrome.ps1",
        "Export-ExactTrustedBlob",
        "Materialized trusted blob does not byte-match FINAL_SHA.",
        "$CanonicalExtensionEntries = @(",
        "$MaximumExtensionEntryBytes = 2MB",
        "$MaximumExtensionPayloadBytes = 8MB",
        "$MaximumExtensionArchiveBytes = 8MB",
        "function Get-ValidatedExtensionZipCentralDirectory(",
        "Extension ZIP contains an encrypted entry.",
        "Extension ZIP uses an unsupported compression method.",
        "Extension ZIP contains a duplicate, traversal-capable, or noncanonical entry name.",
        "Extension ZIP contains a directory, link, or special entry.",
        "Extension ZIP central directory is not the exact canonical eleven-file layout.",
        "Extension ZIP local and central metadata differ or use unsupported metadata.",
        "function New-Crc32Table",
        "function Update-Crc32(",
        "$Crc32State = Update-Crc32 $Crc32State $Buffer $Read $Crc32Table",
        "Extension ZIP entry CRC-32 does not match its central-directory declaration.",
        "function Invoke-BoundedExactExtensionZipExtraction(",
        "[IO.FileShare]::Read",
        "[IO.FileMode]::CreateNew",
        "Extension ZIP entry expanded beyond its declared or allowed byte bound.",
        "Extension ZIP entry did not expand to its declared length.",
        "Extension ZIP hash changed during bounded streaming extraction.",
        "Extension ZIP path identity or hash changed across bounded extraction.",
        "Invoke-BoundedExactExtensionZipExtraction `",
        "CRC-32 known-answer self-test failed.",
        "Stock-Chrome bounded extension ZIP valid fixture failed.",
        "Assert-SelfTestExtractionRejected $CrcArchive \"a CRC-32 mismatch\"",
        "Assert-SelfTestExtractionRejected $OversizedArchive \"an oversized declared entry\"",
        "Assert-SelfTestExtractionRejected $TraversalArchive \"a traversal entry\"",
        "Assert-SelfTestExtractionRejected $DuplicateArchive \"a duplicate entry\"",
        "browser-evidence-candidate.ps1\" -Mode Preflight",
        "record-computer-helper-chain.ps1\" -Mode Run",
        "sanitize-browser-evidence-screenshot.ps1\" -Mode Sanitize",
        "sanitize-browser-evidence-screenshot.ps1\" -Mode BindReview",
        "Invoke-IndependentReviewerExchange",
        "Claim-PublishedReviewerResponse",
        "Assert-UnchangedPrivatePng",
        "Remove-ExactReviewExchangeDirectory",
        "Write-CreateOncePrivateJson",
        "Read-StablePrivateJson",
        "scoped-action-approval.json",
        "independent-visual-review.json",
        "browser-evidence-candidate.ps1\" -Mode Postflight",
        "write-browser-evidence-record.ps1\" -Mode Finalize",
        "Remove-ExactFlatOwnedDirectory $RawScreenshotDirectory",
        "Remove-ExactFlatOwnedDirectory $ExtensionDirectory",
        "Remove-TestOwnedTree $EvidenceDirectory",
        "partialEvidenceDirectoryDeleted",
        "Stock-Chrome sensitive-review failure cleanup retained a PNG or sidecar.",
        "if ($Owned -ceq $EvidenceDirectory -or $Owned -ceq $AttemptDirectory) { continue }",
        "Failed evidence directory: $EvidenceDirectory",
        "Acceptance cleanup failed and passing output was invalidated",
        "Windows stock-Chrome acceptance passed.",
        "Evidence directory: $EvidenceDirectory",
    ] {
        assert!(
            coordinator.contains(required),
            "checked-in stock-Chrome coordinator is missing {required}"
        );
    }
    assert!(!coordinator.contains("REPLACE_WITH_"));
    assert!(!coordinator.contains("GetEnvironmentVariable(\"SystemRoot\", \"Machine\")"));
    assert!(
        !coordinator.contains("[IO.Compression.ZipFile]::ExtractToDirectory"),
        "the coordinator must not use the framework's unbounded ZIP extractor"
    );
    for forbidden in [
        "$env:HOME =",
        "$env:USERPROFILE =",
        "$env:CODEX_HOME =",
        "SetEnvironmentVariable(\"HOME\"",
        "SetEnvironmentVariable(\"USERPROFILE\"",
        "SetEnvironmentVariable(\"CODEX_HOME\"",
    ] {
        assert!(
            !coordinator.contains(forbidden),
            "coordinator must not repurpose a user or Codex home variable: {forbidden}"
        );
    }
    for required in [
        "Write-CreateOnceAttemptRecord",
        "attempt-start.json",
        "failure.json",
        "cleanup.json",
        "FinalEntries.Count -ne 22",
        "[IO.FileMode]::CreateNew",
        "Invoke-ExactPs51SelfTest",
    ] {
        assert!(
            coordinator.contains(required),
            "coordinator is missing hardened attempt or PS5.1 execution contract: {required}"
        );
    }

    let asset_hash = coordinator.find("$ObservedAssetSha =").unwrap();
    let binding = coordinator.find("$Binding =").unwrap();
    let attest = coordinator
        .find("$Info.Arguments = \"attestation verify")
        .unwrap();
    let head = coordinator.find("$ObservedHead =").unwrap();
    let detached = coordinator.find("$SymbolicHead =").unwrap();
    let clean = coordinator.find("$Dirty =").unwrap();
    let ignored = coordinator.find("$Ignored =").unwrap();
    let blob = coordinator.find("$ExpectedBlob =").unwrap();
    let materialized = coordinator
        .find("Export-ExactTrustedBlob $ExpectedBlob $Relative")
        .unwrap();
    let extraction = coordinator
        .rfind("Invoke-BoundedExactExtensionZipExtraction `")
        .unwrap();
    let first_trusted_script = coordinator
        .find("& \"$Scripts\\browser-evidence-candidate.ps1\" -Mode Preflight")
        .unwrap();
    assert!(
        asset_hash < binding
            && binding < attest
            && attest < head
            && head < detached
            && detached < clean
            && clean < ignored
            && ignored < blob
            && blob < materialized
            && materialized < extraction
            && extraction < first_trusted_script,
        "trust, exact-attempt provenance, clean-source, and tagged-blob gates must precede extraction and execution"
    );
    assert_eq!(
        coordinator
            .matches("function Invoke-BoundedExactExtensionZipExtraction(")
            .count(),
        1,
        "the bounded extractor must have one canonical implementation"
    );

    let sanitize = coordinator.rfind("-Mode Sanitize").unwrap();
    let aggregate_review = coordinator.rfind("\"six-crop-review\"").unwrap();
    let bind_review = coordinator.rfind("-Mode BindReview").unwrap();
    assert!(sanitize < aggregate_review && aggregate_review < bind_review);
    assert_eq!(coordinator.matches("Read-Host").count(), 1);
    for obsolete in [
        "Exact purpose-and-image-digest-bound human review receipt",
        "-Mode AttestReview",
        "-ManualVisualReviewConfirmed",
    ] {
        assert!(!coordinator.contains(obsolete));
    }
}

#[test]
fn finalizer_is_allowlisted_and_requires_handback_polling_cleanup_and_review() {
    let script = source("scripts/write-browser-evidence-record.ps1");
    for required in [
        "Assert-ApiMatrixRecord",
        "Assert-CandidatePreflight",
        "Candidate postflight does not bind the supplied preflight record.",
        "Candidate preflight and postflight bindings differ.",
        "$preflightRead = Read-JsonWithSha256 $preflightPath \"PreflightRecord\"",
        "$preflightSha256 = $preflightRead.Sha256",
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
        "Independent visual review must contain exactly six ordered entries.",
        "independentVisualReviewRequired",
        "independentVisualReviewCompleted",
        "reviewRecordSha256",
        "reviewEntryRef",
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
    let v01229_readme = source("evidence/v0.12.41/browser/README.md");
    let coordinator = source("scripts/test-windows-stock-chrome.ps1");
    for required in [
        "LegacyV0122ReviewConfirmed is mandatory for v0.12.2 compatibility.",
        "LegacyV0122ReviewConfirmed is forbidden for the v0.12.41 independent-review protocol.",
        "stock-user-chrome-screenshot-review-pending",
        "$legacyOnePhase = $script:CandidateVersionFromPreflight -ceq \"0.12.2\"",
        "$script:PendingReviewStatement",
        "$script:CompletedReviewStatement",
        "$script:LegacyReviewStatement",
        "$script:CandidateVersionFromPreflight",
        "BindReview is available only for the v0.12.41 independent-review protocol.",
        "The sanitized PNG changed after its pending review record was created.",
        "Screenshot review binding succeeded without an independent review record.",
        "IndependentReviewRecord contains a mismatched, failed, sensitive, uncertain, or reordered entry.",
        "Legacy v0.12.2 screenshot sanitization compatibility failed.",
        "metadataStrippedByDecodeAndReencode = $true",
        "forbiddenMetadataChunksPresent = $false",
        "automaticPixelRedactionPerformed = $false",
        "unknownPixelSafetyClaimed = $false",
        "$script:CompletedReviewStatement",
        "automatedTextInspectionPerformed = $false",
        "independentVisualReviewRequired = $true",
        "independentVisualReviewCompleted = $true",
        "Read-StrictJsonWithDigest",
        "ConvertFrom-JsonPreservingStrings",
        "$command.Parameters.ContainsKey(\"DateKind\")",
        "ConvertFrom-Json -InputObject $Json -DateKind String",
        "[IO.FileShare]::None",
        "Get-PngChunkTypesFromBytes",
        "Get-BytesSha256 $sanitizedBytes",
        "Read-StableBytes $inputPath $script:MaxInputBytes \"InputImage\"",
        "[IO.MemoryStream]::new($inputBytes, $false)",
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
        "extension-loaded",
        "api-action-result",
        "computer-share-action",
        "stop-paused",
        "cancel-paused",
        "post-handback-resume",
    ] {
        assert!(script.contains(&format!("\"{purpose}\"")));
        assert!(finalizer.contains(&format!("\"{purpose}\"")));
    }
    assert!(!script.contains("automaticPixelRedactionPerformed = $true"));
    assert!(!script.contains("unknownPixelSafetyClaimed = $true"));
    assert!(!script.to_ascii_lowercase().contains("tesseract"));
    assert!(!script.contains("popup-release-after"));
    assert!(!script.contains("popup-release-paused-popup"));
    let sanitize = coordinator.rfind("-Mode Sanitize").unwrap();
    let review = coordinator.rfind("\"six-crop-review\"").unwrap();
    let bind = coordinator.rfind("-Mode BindReview").unwrap();
    assert!(sanitize < review && review < bind);
    assert!(!coordinator.contains("-ManualVisualReviewConfirmed"));
    assert!(v01229_readme.contains("six digest-bound finalized screenshot sidecars"));
}

#[test]
fn operator_protocol_is_stock_chrome_autonomous_and_fail_closed() {
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
    let v01229_readme = source("evidence/v0.12.41/browser/README.md");

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
    let normalized_v01229 = v01229_readme
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(normalized_v01229.contains("accept the popup confirmation under the scoped approval"));
    assert!(v01229_readme.contains("Chrome's token-clear confirmation"));
    assert!(!v01229_readme.contains("confirmationAcceptedByHuman"));
}

#[test]
fn v01229_computer_helper_chain_is_live_exact_window_and_six_capture_bound() {
    let recorder = source("scripts/record-computer-helper-chain.ps1");
    let finalizer = source("scripts/write-browser-evidence-record.ps1");
    let schema: Value = serde_json::from_str(&source(
        "evidence/v0.12.41/browser/computer-helper-chain.schema.json",
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
        "Assert-PrivateOperatorExchangeDirectory",
        "Read-StableJsonWithDigest",
        "Write-CreateOnceJson",
        "Save-LoopbackBinaryCreateOnce",
        "[IO.FileMode]::CreateNew",
        "[IO.FileShare]::None",
        "Only the scoped approval request may use the user-via-orchestrator responder role.",
        "The scoped user approval did not use the exact preflight attestor session.",
        "The operator exchange changed independent reviewer sessions mid-run.",
        "Timed out waiting for the atomically published create-once operator response.",
        "Claim-PublishedOperatorResponse",
        "Assert-UnchangedOperatorFrame",
        "Operator exchange cleanup found an unregistered, missing, extra, or replaced reservation artifact.",
        "Confirm-ApprovalPreDispatchStateUnchanged",
        "Post-approval state revalidation reused the approval challenge frame.",
        "Candidate absence, Developer Mode, or dedicated-window state changed after approval.",
        "$script:FirstCoveredActionDispatchedAtUtc = $dispatchedAt",
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

    let raw_cleanup = recorder
        .split("function Remove-CanonicalRawScreenshots")
        .nth(1)
        .unwrap()
        .split("function Get-CandidateBinding")
        .next()
        .unwrap();
    for required in [
        "foreach ($name in $script:Screenshots.Values)",
        "[IO.Path]::Combine($directory.FullName, $name)",
        "[IO.File]::Delete($path)",
        "Canonical raw screenshot cleanup was incomplete",
    ] {
        assert!(
            raw_cleanup.contains(required),
            "raw screenshot cleanup is missing {required}"
        );
    }
    assert!(!raw_cleanup.contains("GetFileSystemInfos"));
    assert!(!raw_cleanup.contains("[IO.Directory]::Delete"));

    let final_record_write = recorder
        .split("$temporary = \"$outputPath.new\"")
        .nth(1)
        .unwrap()
        .split("Write-Output \"Live candidate-bound computer-helper chain record created.\"")
        .next()
        .unwrap();
    let move_index = final_record_write
        .find("[IO.File]::Move($temporary, $outputPath)")
        .unwrap();
    let catch_index = final_record_write.find("catch {").unwrap();
    let cleanup_index = final_record_write
        .find("Remove-CanonicalRawScreenshots $script:RawDirectory")
        .unwrap();
    let finally_index = final_record_write.rfind("finally {").unwrap();
    let temporary_cleanup_index = final_record_write
        .find("[IO.File]::Delete($temporary)")
        .unwrap();
    assert!(move_index < catch_index && catch_index < cleanup_index);
    assert!(cleanup_index < finally_index && finally_index < temporary_cleanup_index);
    assert!(final_record_write.contains("Canonical raw screenshot cleanup completed."));
    assert!(final_record_write.contains("Final-record failure cleanup was incomplete:"));
    assert!(final_record_write.contains("[IO.FileMode]::CreateNew"));
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

    assert_eq!(schema["properties"]["screenshots"]["minItems"], 6);
    assert_eq!(schema["properties"]["screenshots"]["maxItems"], 6);
    assert_eq!(schema["properties"]["windowEpochs"]["minItems"], 13);
    assert_eq!(schema["properties"]["windowEpochs"]["maxItems"], 13);
    assert_eq!(schema["properties"]["actions"]["minItems"], 27);
    assert_eq!(schema["properties"]["actions"]["maxItems"], 27);
    assert_eq!(schema["properties"]["schemaVersion"]["const"], 2);
    assert_eq!(
        schema["properties"]["operatorExchange"]["properties"]["allRequestsCreateOnce"]["const"],
        true
    );
    assert_eq!(
        schema["properties"]["operatorExchange"]["properties"]["everyFrameDependentDecisionBoundToFreshFrame"]
            ["const"],
        true
    );
    assert_eq!(
        schema["properties"]["scopedActionApproval"]["properties"]["consumedBeforeFirstCoveredAction"]
            ["const"],
        true
    );
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
        "The computer-helper record must bind exactly six raw exact-window screenshots.",
        "computer-helper exact-image direct-child count",
        "The exact helper worker was not protocol-bound through computer.status.",
        "candidateCardAbsenceVerifiedFromFreshLiveUi",
        "canonicalPortListenersRemaining",
        "A sanitized screenshot sidecar does not bind its exact helper-captured raw PNG.",
        "Evidence finalizer accepted a different dedicated Chrome window in a later epoch.",
        "Evidence finalizer accepted a dedicated window without a sole-new status delta.",
        "Evidence finalizer accepted a scoped approval at or after the first covered action.",
        "Evidence finalizer accepted a reused operator decision reference.",
        "Evidence finalizer accepted an approval delivered through the executor session.",
        "Evidence finalizer accepted an approval delivered through the reviewer session.",
    ] {
        assert!(
            finalizer.contains(required),
            "helper finalizer is missing {required}"
        );
    }
}

#[test]
fn v01229_powershell_entrypoints_cannot_fall_through_legacy_gates() {
    let paths = [
        "scripts/browser-evidence-candidate.ps1",
        "scripts/record-computer-helper-chain.ps1",
        "scripts/sanitize-browser-evidence-screenshot.ps1",
        "scripts/test-windows-browser-api.ps1",
        "scripts/test-windows-computer-use.ps1",
        "scripts/wait-windows-foreground-arm-handoff.ps1",
        "scripts/write-browser-evidence-record.ps1",
        "scripts/write-stock-chrome-operator-response.ps1",
    ];
    for path in paths {
        assert!(
            !source(path).contains("0.12.9"),
            "current PowerShell entrypoint retained the prior candidate gate: {path}"
        );
    }

    let candidate = source("scripts/browser-evidence-candidate.ps1");
    for required in [
        "$Version -ceq \"0.12.41\"",
        "$ExpectedVersion -ceq \"0.12.41\"",
        "$testVersion = \"0.12.2\"",
        "Preserve the v0.12.2 contract",
    ] {
        assert!(
            candidate.contains(required),
            "candidate gate is missing {required}"
        );
    }

    let sanitizer = source("scripts/sanitize-browser-evidence-screenshot.ps1");
    assert!(sanitizer.contains("@(\"0.12.2\", \"0.12.41\")"));
    assert!(sanitizer.contains("$script:CandidateVersionFromPreflight -ceq \"0.12.41\""));
    let run_fields = sanitizer
        .find("workflowRunId = \"1\"; workflowRunAttempt = \"1\"; workflowEvent = \"workflow_dispatch\"")
        .expect("sanitizer self-test must order workflow fields canonically");
    let artifact_fields = sanitizer[run_fields..]
        .find("artifactId = \"1\"; artifactName = \"release-candidate\"")
        .expect("sanitizer self-test must place artifact fields after workflow fields");
    assert!(artifact_fields > 0);

    let finalizer = source("scripts/write-browser-evidence-record.ps1");
    assert!(finalizer.contains("$script:OperatorV2Version = \"0.12.41\""));
    assert!(finalizer.contains("@(\"0.12.2\", $script:OperatorV2Version)"));
    assert!(finalizer.contains("\"evidence\", \"v0.12.41\", \"browser\""));

    let response_writer = source("scripts/write-stock-chrome-operator-response.ps1");
    let response_run_fields = response_writer
        .find("workflowRunId = \"123\"; workflowRunAttempt = \"1\"; workflowEvent = \"workflow_dispatch\"")
        .expect("response-writer self-test must order workflow fields canonically");
    let response_artifact_fields = response_writer[response_run_fields..]
        .find("artifactId = \"456\"; artifactName = \"release-candidate\"")
        .expect("response-writer self-test must place artifact fields after workflow fields");
    assert!(response_artifact_fields > 0);

    let recorder = source("scripts/record-computer-helper-chain.ps1");
    assert!(recorder.contains("$script:Version = \"0.12.41\""));

    let browser_runner = source("scripts/test-windows-browser-api.ps1");
    assert!(browser_runner.contains("$Version -ceq \"0.12.41\""));
    for required in [
        "preflight-release-candidate-binding",
        "Assert-ReleaseCandidateBinding",
        "workflowRunAttempt",
        "self-test-release-attempt-mismatch-rejected",
        "Assert-NoReparseAncestorChain $resolved \"preflight-record\"",
        "Assert-NoReparseAncestorChain $outputParent \"output-parent\"",
    ] {
        assert!(
            browser_runner.contains(required),
            "browser API runner is missing exact V2 preflight binding contract: {required}"
        );
    }

    let computer_runner = source("scripts/test-windows-computer-use.ps1");
    assert!(computer_runner.contains("-ProductVersion \"0.12.41\""));
    assert!(computer_runner.contains("productVersion -cne \"0.12.41\""));

    let watcher = source("scripts/wait-windows-foreground-arm-handoff.ps1");
    assert!(watcher.contains("$script:ProductVersion = \"0.12.41\""));
    assert!(watcher.contains("$script:MarkerSchemaVersion = 2"));
    assert!(watcher.contains("productVersion = \"0.12.41\""));
}

#[test]
fn v01229_powershell_json_readers_preserve_iso_timestamp_strings() {
    for path in [
        "scripts/browser-evidence-candidate.ps1",
        "scripts/record-computer-helper-chain.ps1",
        "scripts/sanitize-browser-evidence-screenshot.ps1",
        "scripts/test-windows-browser-api.ps1",
        "scripts/test-windows-computer-use.ps1",
        "scripts/test-windows-stock-chrome.ps1",
        "scripts/write-browser-evidence-record.ps1",
        "scripts/write-stock-chrome-operator-response.ps1",
    ] {
        let script = source(path);
        for required in [
            "function ConvertFrom-JsonPreservingStrings",
            "ValueFromPipeline = $true",
            "Get-Command ConvertFrom-Json -CommandType Cmdlet",
            "$command.Parameters.ContainsKey(\"DateKind\")",
            "Microsoft.PowerShell.Utility\\ConvertFrom-Json -InputObject $Json -DateKind String",
        ] {
            assert!(
                script
                    .to_ascii_lowercase()
                    .contains(&required.to_ascii_lowercase()),
                "PowerShell JSON reader must preserve ISO timestamp strings on PS 7.5+: {path} missing {required}"
            );
        }
    }
}

#[test]
fn v01238_powershell_acl_paths_select_the_supported_runtime_api() {
    for path in [
        "scripts/record-computer-helper-chain.ps1",
        "scripts/test-windows-stock-chrome.ps1",
        "scripts/verify-windows-release-candidate.ps1",
        "scripts/write-stock-chrome-operator-response.ps1",
    ] {
        let script = source(path);
        for required in [
            "$PSVersionTable.PSEdition -ceq \"Core\"",
            "[IO.FileSystemAclExtensions]::GetAccessControl",
            "[IO.Directory]::GetAccessControl",
        ] {
            assert!(
                script.contains(required),
                "PowerShell ACL reader must select the runtime-supported API: {path} missing {required}"
            );
        }
    }

    for path in [
        "scripts/test-windows-stock-chrome.ps1",
        "scripts/write-stock-chrome-operator-response.ps1",
    ] {
        let script = source(path);
        for required in [
            "[IO.FileSystemAclExtensions]::SetAccessControl",
            "[IO.Directory]::SetAccessControl",
        ] {
            assert!(
                script.contains(required),
                "PowerShell ACL writer must select the runtime-supported API: {path} missing {required}"
            );
        }
    }

    let verifier = source("scripts/verify-windows-release-candidate.ps1");
    for required in [
        "function New-PrivateDirectory([string]$Path)",
        "[IO.FileSystemAclExtensions]::Create(",
        "[IO.Directory]::CreateDirectory($Path, $Security)",
        "New-PrivateDirectory $Root",
        "New-PrivateDirectory $Destination",
    ] {
        assert!(
            verifier.contains(required),
            "Windows trust verifier must create its protected directory atomically: missing {required}"
        );
    }
    assert!(!verifier.contains("Set-DirectoryAccessControlPortable"));
    assert!(!verifier.contains("[IO.Directory]::SetAccessControl"));
    assert!(!verifier.contains("[IO.FileSystemAclExtensions]::SetAccessControl"));
}

#[test]
fn v01229_named_pipe_relay_uses_runtime_acl_factory_and_nonmasking_cleanup() {
    let script = source("scripts/write-stock-chrome-operator-response.ps1");
    for required in [
        "$PSVersionTable.PSEdition -ceq \"Core\"",
        "[IO.Pipes.NamedPipeServerStreamAcl]::Create(",
        "[IO.Pipes.NamedPipeServerStream]::new(",
        "[IO.HandleInheritability]::None",
        "[IO.Pipes.PipeAccessRights]0",
    ] {
        assert!(
            script.contains(required),
            "named-pipe relay is missing runtime-compatible ACL creation: {required}"
        );
    }
    assert!(!script.contains("$ConnectTask.Dispose()"));
    assert!(!script.contains("$Flush.Dispose()"));
    assert!(!script.contains("AsyncWaitHandle"));
    assert!(!script.contains("BeginWaitForConnection"));
    assert!(!script.contains("BeginRead"));
    assert!(!script.contains("BeginWrite"));
    for required in [
        "$Pipe.WaitForConnectionAsync()",
        "$InputStream.ReadAsync($OneByte, 0, 1)",
        "$Pipe.WriteAsync($OneByte, 0, 1)",
        "$AcceptTask.GetAwaiter().GetResult()",
        "$ReadTask.GetAwaiter().GetResult()",
        "$WriteTask.GetAwaiter().GetResult()",
        "$Flush.GetAwaiter().GetResult()",
    ] {
        assert!(
            script.contains(required),
            "named-pipe relay is missing task-safe asynchronous I/O: {required}"
        );
    }

    let consumer = source("scripts/test-windows-stock-chrome.ps1");
    let reader_start = consumer
        .find("function Read-GitHubAcceptanceToken")
        .expect("credential-pipe reader is missing");
    let reader_end = consumer[reader_start..]
        .find("$PrimaryFailure = $null")
        .map(|offset| reader_start + offset)
        .expect("credential-pipe reader boundary is missing");
    let reader = &consumer[reader_start..reader_end];
    assert!(!reader.contains("AsyncWaitHandle"));
    assert!(!reader.contains("BeginRead"));
    assert!(reader.contains("$Pipe.ReadAsync($OneByte, 0, 1)"));
    assert!(reader.contains("$ReadTask.GetAwaiter().GetResult()"));

    let relay_start = script
        .find("$RelayName = \"lbb-gh-\"")
        .expect("named-pipe relay self-test is missing");
    let relay_end = script[relay_start..]
        .find("foreach ($EscapingName")
        .map(|offset| relay_start + offset)
        .expect("named-pipe relay self-test boundary is missing");
    let relay = &script[relay_start..relay_end];
    assert!(script.contains("foreach ($RelayIteration in 1..2)"));

    let client_dispose = relay
        .find("$RelayClient.Dispose()")
        .expect("relay client cleanup is missing");
    let pending_guard = relay
        .find("if (-not $ConnectTask.IsCompleted)")
        .expect("pending connect-task guard is missing");
    let bounded_wait = relay
        .find("try { [void]$ConnectTask.Wait(1000) } catch {}")
        .expect("pending connect-task cleanup is not bounded");

    assert!(client_dispose < pending_guard);
    assert!(pending_guard < bounded_wait);
}

#[test]
fn new_browser_evidence_files_are_english_only() {
    for path in [
        "scripts/browser-evidence-candidate.ps1",
        "scripts/sanitize-browser-evidence-screenshot.ps1",
        "scripts/test-windows-browser-api.ps1",
        "scripts/test-windows-stock-chrome.ps1",
        "scripts/verify-windows-release-candidate.ps1",
        "scripts/verify-release-acceptance-evidence.sh",
        "scripts/fetch-verify-release-candidate.sh",
        "scripts/finalize-macos-acceptance.mjs",
        "scripts/write-browser-evidence-record.ps1",
        "scripts/record-computer-helper-chain.ps1",
        "scripts/run-windows-computer-use-acceptance.ps1",
        "scripts/write-stock-chrome-operator-response.ps1",
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
        "evidence/v0.12.9/browser/README.md",
        "evidence/v0.12.9/browser/operator-results.template.json",
        "evidence/v0.12.9/browser/operator-results.schema.json",
        "evidence/v0.12.9/browser/computer-helper-chain.schema.json",
        "evidence/v0.12.27/browser/README.md",
        "evidence/v0.12.27/browser/operator-results.template.json",
        "evidence/v0.12.27/browser/operator-results.schema.json",
        "evidence/v0.12.27/browser/computer-helper-chain.schema.json",
        "evidence/v0.12.27/browser/scoped-action-approval.schema.json",
        "evidence/v0.12.27/browser/independent-visual-review.schema.json",
        "evidence/v0.12.27/browser/external-surface-attestation.schema.json",
        "evidence/v0.12.41/browser/README.md",
        "evidence/v0.12.41/browser/operator-results.template.json",
        "evidence/v0.12.41/browser/operator-results.schema.json",
        "evidence/v0.12.41/browser/computer-helper-chain.schema.json",
        "evidence/v0.12.41/browser/scoped-action-approval.schema.json",
        "evidence/v0.12.41/browser/independent-visual-review.schema.json",
        "evidence/v0.12.41/browser/external-surface-attestation.schema.json",
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

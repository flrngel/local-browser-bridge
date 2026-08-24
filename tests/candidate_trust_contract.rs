use std::fs;

fn normalized_source(path: &str) -> String {
    fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

#[test]
fn windows_candidate_binder_is_a_clean_child_binary_safe_fail_closed_gate() {
    let script = normalized_source("scripts/verify-windows-release-candidate.ps1");

    for required in [
        "#requires -Version 5.1",
        "[string]$Version",
        "[string]$WorkflowRunId",
        "[string]$WorkflowRunAttempt",
        "[string]$ArtifactId",
        "[string]$SourceSha",
        "[string]$TagObjectSha",
        "[string]$Destination",
        "[string]$TrustedGit",
        "[string]$TrustedGh",
        "[switch]$SelfTest",
        "System32\", \"WindowsPowerShell\", \"v1.0\", \"powershell.exe",
        "Sysnative\", \"WindowsPowerShell\", \"v1.0\", \"powershell.exe",
        "[Microsoft.Win32.RegistryView]::Registry64",
        "[Microsoft.Win32.RegistryKey]::OpenBaseKey",
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
        "-NoLogo -NoProfile -File",
        "[Environment]::Is64BitProcess",
        "MainModule.FileName",
        "LBB_WINDOWS_TRUST_NONCE",
        "The trust body must run only in its exact self-spawned 64-bit system powershell.exe child.",
        "Windows release-candidate trust wrapper self-test passed.",
        "Self-test cleanup refused an unexpected or linked inventory.",
        "Self-test stops here: no candidate path, network client, token, archive, or",
        "Destination must be a fresh short path",
        "SetAccessRuleProtection($true, $false)",
        "Fresh destination ACL is not private to the current user.",
        "BaseStream.CopyToAsync($Output)",
        "Raw artifact ZIP size mismatch.",
        "Raw artifact ZIP SHA-256 mismatch.",
        "$MaximumCandidateBytes = [int64]536870912",
        "actions/runs/$WorkflowRunId/attempts/$WorkflowRunAttempt",
        "actions/runs/$WorkflowRunId/attempts/$WorkflowRunAttempt/jobs?per_page=100",
        "Assemble frozen release candidate",
        "Exact-attempt workflow does not contain one successful bounded assembly job.",
        "repos/$Repository/actions/artifacts/$ArtifactId",
        "Direct release-candidate artifact metadata mismatch.",
        "Direct artifact metadata is not bound to the successful current-attempt assembly job.",
        "$ArtifactsResponse.total_count -ge 100",
        "exactly one expected nonexpired release-candidate artifact",
        "Outer artifact ZIP is not the exact flat five-file candidate.",
        "Outer artifact ZIP exceeds the bounded uncompressed candidate size.",
        "Copy-ZipEntryBounded $Entry[0] $OutputStream $MaximumCandidateBytes",
        "Archive entry expanded beyond its declared or allowed size.",
        "SHA256SUMS.txt must contain exactly four LF-terminated rows.",
        "SHA256SUMS.txt row order or spelling is not canonical.",
        "Candidate payload SHA-256 mismatch.",
        "$Reader.ReadUInt16() -ne 0x8664",
        "$Reader.ReadUInt16() -ne 0x020b",
        "Assert-ExtensionArchive",
        "Extension archive contains an unexpected, linked, or oversized entry.",
        "Extension archive version does not match the release candidate.",
        "Assert-MacosArchive",
        "macOS tar PAX metadata could alter extraction semantics.",
        "macOS tar logical inventory is unexpected or duplicated.",
        "macOS tar executable is not an executable universal Mach-O.",
        "macOS tar contains more than one MiB of terminal padding.",
        "clone\", \"--no-checkout\", \"--no-local",
        "core.longpaths=true",
        "checkout\", \"--detach\", \"--force\", $SourceSha",
        "status\", \"--porcelain=v2\", \"--untracked-files=all",
        "ls-files\", \"--deleted",
        "ls-files\", \"--others\", \"--exclude-standard",
        "diff\", \"--quiet\", \"HEAD\", \"--",
        "diff\", \"--cached\", \"--quiet",
        "fsck\", \"--full",
        "scripts/verify-windows-release-candidate.ps1",
        "scripts/test-windows-computer-use.ps1",
        "tests/fixtures/windows/WindowsComputerUseFixture.ps1",
        "rev-parse\", \"--verify\", \"HEAD:$Relative",
        "hash-object\", \"--no-filters\", \"--\", $Relative",
        "Executing trust wrapper does not match the exact tagged wrapper blob.",
        "\"attestation\", \"verify\"",
        "--deny-self-hosted-runners\", \"--format\", \"json",
        "$Predicate.runDetails.metadata.invocationId -cne $ExpectedInvocationUri",
        "$Certificate.runInvocationURI -cne $ExpectedInvocationUri",
        "$Certificate.githubWorkflowSHA -cne $SourceSha",
        "$Certificate.githubWorkflowRepository -cne $Repository",
        "$Certificate.runnerEnvironment -cne \"github-hosted\"",
        "$Certificate.sourceRepositoryDigest -cne $SourceSha",
        "Write-CreateOnceUtf8Json $BindingPath $Binding",
    ] {
        assert!(
            script.contains(required),
            "Windows candidate binder is missing `{required}`"
        );
    }

    let self_test_stop = script.find("if ($SelfTestRequested)").unwrap();
    let token_read = script.find("$InheritedToken =").unwrap();
    let source_clone = script.find("Fixed-origin fresh source clone").unwrap();
    let token_prompt = script
        .find("Read-Host \"Least-privilege GitHub candidate-verification token\"")
        .unwrap();
    let first_api = script.find("$Run = Invoke-TrustedGhJson").unwrap();
    let raw_download = script.find("Invoke-TrustedGhBinary @(").unwrap();
    let raw_digest = script.find("Raw artifact ZIP SHA-256 mismatch.").unwrap();
    let archive_open = raw_digest
        + script[raw_digest..]
            .find("[IO.Compression.ZipFile]::OpenRead")
            .unwrap();
    let pe_check = script
        .rfind("Assert-Pe32PlusX64 (Join-Path $PayloadDirectory")
        .unwrap();
    let attestation = script.rfind("\"attestation\", \"verify\"").unwrap();
    let binding = script
        .rfind("Write-CreateOnceUtf8Json $BindingPath")
        .unwrap();
    assert!(self_test_stop < token_read);
    assert!(token_read < source_clone);
    assert!(source_clone < token_prompt);
    assert!(token_prompt < first_api);
    assert!(first_api < raw_download);
    assert!(raw_download < raw_digest);
    assert!(raw_digest < archive_open);
    assert!(archive_open < pe_check);
    assert!(pe_check < attestation);
    assert!(attestation < binding);

    assert_eq!(
        script
            .matches("Invoke-TrustedProcessText $TrustedGh @(")
            .count(),
        1,
        "all five attestations must flow through one exact loop"
    );
    assert!(script.contains("foreach ($Name in $ExpectedFiles)"));
    assert!(script.contains("foreach ($Attestation in $Attestations)"));
    assert!(script.contains("$ExpectedArtifactBytes = [int64]$DirectArtifact.size_in_bytes"));
    assert!(
        script.contains("$ExpectedArtifactSha256 = ([string]$DirectArtifact.digest).Substring(7)")
    );
}

#[test]
fn dedicated_windows_fixture_is_trusted_source_only_not_a_release_asset() {
    let windows_binder = normalized_source("scripts/verify-windows-release-candidate.ps1");
    let bash_binder = normalized_source("scripts/fetch-verify-release-candidate.sh");
    let release = normalized_source(".github/workflows/deploy.yml");
    let asset_verifier = normalized_source("scripts/verify-release-assets.sh");

    let windows_assets = windows_binder
        .split("$ExpectedAssets = @(")
        .nth(1)
        .unwrap()
        .split("$ExpectedFiles = @($ExpectedAssets) + \"SHA256SUMS.txt\"")
        .next()
        .unwrap();
    for expected in [
        "local-browser-bridge-v$Version-windows-x86_64.exe",
        "local-computer-helper-v$Version-windows-x86_64.exe",
        "local-browser-bridge-v$Version-macos-universal.tar.gz",
        "local-browser-bridge-extension-v$Version.zip",
    ] {
        assert!(windows_assets.contains(expected));
    }
    assert_eq!(
        windows_assets
            .lines()
            .filter(|line| line.trim_start().starts_with("\"local-"))
            .count(),
        4,
        "the Windows candidate binder must expose exactly four product assets"
    );
    assert!(!windows_assets.to_ascii_lowercase().contains("fixture"));
    assert!(windows_binder.contains("attestedAssetCount = 5"));

    let trusted_source = windows_binder
        .split("$TrustedRelativeFiles = @(")
        .nth(1)
        .unwrap()
        .split("foreach ($Relative in $TrustedRelativeFiles)")
        .next()
        .unwrap();
    assert!(trusted_source.contains("tests/fixtures/windows/WindowsComputerUseFixture.ps1"));
    assert!(
        windows_binder.contains(
            "$Blob = Invoke-SourceGit @(\"rev-parse\", \"--verify\", \"HEAD:$Relative\")"
        )
    );
    assert!(windows_binder.contains(
        "$WorktreeBlob = Invoke-SourceGit @(\"hash-object\", \"--no-filters\", \"--\", $Relative)"
    ));
    assert!(
        windows_binder
            .contains("Required wrapper, runner, or fixture does not match its exact source blob.")
    );

    let bash_assets = bash_binder
        .split("ASSETS=(")
        .nth(1)
        .unwrap()
        .split("RELEASE_FILES=(\"${ASSETS[@]}\" \"SHA256SUMS.txt\")")
        .next()
        .unwrap();
    for expected in [
        "local-browser-bridge-v$VERSION-windows-x86_64.exe",
        "local-computer-helper-v$VERSION-windows-x86_64.exe",
        "local-browser-bridge-v$VERSION-macos-universal.tar.gz",
        "local-browser-bridge-extension-v$VERSION.zip",
    ] {
        assert!(bash_assets.contains(expected));
    }
    assert_eq!(
        bash_assets
            .lines()
            .filter(|line| line.trim_start().starts_with("\"local-"))
            .count(),
        4,
        "the cross-platform candidate binder must expose exactly four product assets"
    );
    assert!(!bash_assets.to_ascii_lowercase().contains("fixture"));
    assert!(bash_binder.contains("attestedAssetCount:5"));

    let assembled_assets = release
        .split("          assets=(\n")
        .nth(1)
        .unwrap()
        .split("          )\n")
        .next()
        .unwrap();
    assert_eq!(
        assembled_assets
            .lines()
            .filter(|line| line.trim_start().starts_with("\"local-"))
            .count(),
        4,
        "release assembly must contain exactly four product assets before the manifest"
    );
    assert!(!assembled_assets.to_ascii_lowercase().contains("fixture"));
    assert!(release.contains("(cd dist && sha256sum \"${assets[@]}\" > SHA256SUMS.txt)"));
    assert!(release.contains("name: release-candidate\n          path: dist/*"));
    assert!(release.contains(
        "$fixtureExecutableSelfTest = Join-Path $env:RUNNER_TEMP (\"lbb-windows-fixture-\""
    ));
    assert!(!release.contains("dist/lbb-windows-fixture-"));
    assert!(!release.contains("dist/lbb-windows-computer-use-fixture"));
    assert!(!release.contains("Copy-Item $fixtureExecutableSelfTest"));

    for required in [
        "windows_server=\"$assets_dir/local-browser-bridge-v${version}-windows-x86_64.exe\"",
        "windows_helper=\"$assets_dir/local-computer-helper-v${version}-windows-x86_64.exe\"",
        "macos_archive=\"$assets_dir/local-browser-bridge-v${version}-macos-universal.tar.gz\"",
        "extension_archive=\"$assets_dir/local-browser-bridge-extension-v${version}.zip\"",
        "checksum_manifest=\"$assets_dir/SHA256SUMS.txt\"",
        "assets=(\"$windows_server\" \"$windows_helper\" \"$macos_archive\" \"$extension_archive\")",
        "$(basename \"$checksum_manifest\")",
        "Release directory contains an unexpected file set.",
    ] {
        assert!(
            asset_verifier.contains(required),
            "release five-file verifier is missing: {required}"
        );
    }
    let release_listing = asset_verifier
        .split("expected_release_listing=\"")
        .nth(1)
        .unwrap()
        .split("actual_release_listing=")
        .next()
        .unwrap();
    assert!(!release_listing.to_ascii_lowercase().contains("fixture"));
}

#[test]
fn windows_candidate_binder_does_not_leak_tokens_or_execute_candidate_bytes() {
    let script = normalized_source("scripts/verify-windows-release-candidate.ps1");

    for required in [
        "Read-Host \"Least-privilege GitHub candidate-verification token\" -AsSecureString",
        "[Environment]::SetEnvironmentVariable(\"GH_TOKEN\", $null, \"Process\")",
        "$Info.EnvironmentVariables[\"GH_TOKEN\"] = $ChildToken",
        "$Info.EnvironmentVariables.Remove(\"GH_TOKEN\")",
        "[Runtime.InteropServices.Marshal]::ZeroFreeBSTR($Bstr)",
        "RedirectStandardOutput = $true",
        "RedirectStandardError = $true",
        "[IO.FileMode]::CreateNew",
    ] {
        assert!(
            script.contains(required),
            "secret-safety contract is missing `{required}`"
        );
    }

    for forbidden in [
        "$Info.Arguments = $ChildToken",
        "$Info.Arguments += $ChildToken",
        "Write-Output $ChildToken",
        "Write-Host $ChildToken",
        "ConvertFrom-SecureString",
        "[Environment]::GetEnvironmentVariable(\"SystemRoot\", \"Machine\")",
        "Set-Content $ArtifactPartial",
        "Out-File $ArtifactPartial",
        "Invoke-WebRequest",
        "Expand-Archive",
        "[IO.Directory]::Delete($Root, $true)",
        "Start-Process (Join-Path $PayloadDirectory",
        "& (Join-Path $PayloadDirectory",
        ".FileName = (Join-Path $PayloadDirectory",
        "chmod +x",
        "--version",
        "$env:HOME =",
        "$env:USERPROFILE =",
        "$env:CODEX_HOME =",
    ] {
        assert!(
            !script.contains(forbidden),
            "Windows trust gate must not contain `{forbidden}`"
        );
    }
}

#[test]
fn bash_and_windows_candidate_binding_schemas_stay_identical() {
    let bash = normalized_source("scripts/fetch-verify-release-candidate.sh");
    let windows = normalized_source("scripts/verify-windows-release-candidate.ps1");
    let fields = [
        "schemaVersion",
        "productVersion",
        "repository",
        "tag",
        "sourceSha",
        "tagObjectSha",
        "workflowRunId",
        "workflowRunAttempt",
        "artifactId",
        "artifactName",
        "artifactZipBytes",
        "artifactZipSha256",
        "checksumManifestSha256",
        "attestationInvocationUri",
        "attestedAssetCount",
        "githubHostedRunner",
        "assets",
        "passed",
    ];

    let bash_binding = bash.rfind("'{schemaVersion:1").unwrap();
    let windows_binding = windows.rfind("$Binding = [ordered]@{").unwrap();
    let mut bash_cursor = bash_binding;
    let mut windows_cursor = windows_binding;
    for field in fields {
        let bash_offset = bash[bash_cursor..]
            .find(field)
            .unwrap_or_else(|| panic!("Bash binding is missing `{field}`"));
        let windows_offset = windows[windows_cursor..]
            .find(field)
            .unwrap_or_else(|| panic!("Windows binding is missing `{field}`"));
        bash_cursor += bash_offset + field.len();
        windows_cursor += windows_offset + field.len();
    }
}

#[test]
fn candidate_binder_is_source_attempt_artifact_and_attestation_bound() {
    let script = fs::read_to_string("scripts/fetch-verify-release-candidate.sh").unwrap();

    for required in [
        "${BASH_SOURCE[0]}",
        "candidate trust script must be executed through its canonical path without symlink traversal",
        "candidate trust script must be one ordinary, singly linked file",
        "candidate trust script must be current-user owned and not group/other writable",
        "git -C \"$SCRIPT_DIRECTORY\" rev-parse --show-toplevel",
        "Candidate trust script is not executing from its canonical source-tree location.",
        "status --porcelain=v2 --untracked-files=all",
        "rev-parse --abbrev-ref HEAD",
        "diff --quiet HEAD --",
        "diff --cached --quiet",
        "ls-files --deleted",
        "ls-files --others --exclude-standard",
        "fsck --full",
        "HEAD:$SCRIPT_RELATIVE",
        "hash-object -- \"$SCRIPT_PATH\"",
        "candidate destination and parent must use canonical paths without symlink traversal",
        "candidate destination parent must be owned by the current user with mode 0700",
        "candidate destination ancestry contains an unprotected writable directory",
        "assert_destination_identity",
        "candidate destination identity changed during verification",
        "actions/runs/$RUN_ID/attempts/$RUN_ATTEMPT",
        "actions/runs/$RUN_ID/attempts/$RUN_ATTEMPT/jobs?per_page=100",
        "actions/runs/$RUN_ID/artifacts?per_page=100",
        "actions/artifacts/$ARTIFACT_ID\" > \"$ARTIFACT_API_JSON",
        "Assemble frozen release candidate",
        "direct artifact metadata is not bound to the successful current-attempt assemble job",
        "release-candidate-artifact-$ARTIFACT_ID.zip",
        "EXPECTED_ARTIFACT_BYTES",
        "EXPECTED_ARTIFACT_SHA256",
        "python3 - \"$ARTIFACT_ZIP\" \"$PAYLOAD_DIRECTORY\"",
        "outer artifact ZIP inventory changed before bounded extraction",
        "RELEASE_FILES=(\"${ASSETS[@]}\" \"SHA256SUMS.txt\")",
        "shasum -a 256 -c SHA256SUMS.txt",
        "verify-release-assets.sh",
        "gh attestation verify",
        "--source-ref \"refs/tags/$TAG\"",
        "--source-digest \"$SOURCE_SHA\"",
        "--signer-workflow \"$REPOSITORY/.github/workflows/deploy.yml\"",
        "--deny-self-hosted-runners",
        "--format json",
        ".verificationResult.statement.predicate.runDetails.metadata.invocationId",
        ".verificationResult.signature.certificate.runInvocationURI",
        "runnerEnvironment == \"github-hosted\"",
        "artifactZipSha256:$artifact_sha256",
        "checksumManifestSha256:$manifest_sha256",
        "passed:true",
    ] {
        assert!(
            script.contains(required),
            "candidate binder is missing {required}"
        );
    }

    let raw_download = script.find("actions/artifacts/$ARTIFACT_ID/zip").unwrap();
    let raw_digest = script.find("raw artifact ZIP digest mismatch").unwrap();
    let extraction = script
        .find("python3 - \"$ARTIFACT_ZIP\" \"$PAYLOAD_DIRECTORY\"")
        .unwrap();
    let inventory = script
        .find("outer artifact ZIP inventory changed before bounded extraction")
        .unwrap();
    let attestation = script.find("gh attestation verify").unwrap();
    let binding = script.rfind("candidate-binding.json").unwrap();
    assert!(raw_download < raw_digest);
    assert!(raw_digest < extraction);
    assert!(extraction < inventory);
    assert!(extraction < attestation);
    assert!(attestation < binding);

    for required in [
        "maximum_entry_bytes = 256 * 1024 * 1024",
        "maximum_total_bytes = 512 * 1024 * 1024",
        "outer artifact ZIP contains duplicate entries",
        "outer artifact ZIP exceeds the bounded uncompressed candidate size",
        "os.O_EXCL | getattr(os, \"O_NOFOLLOW\", 0)",
        "outer artifact ZIP entry expanded beyond its declared size",
    ] {
        assert!(
            script.contains(required),
            "candidate trust binder bounded extraction is missing `{required}`"
        );
    }

    for forbidden in [
        "chmod +x \"$PAYLOAD_DIRECTORY",
        "--no-update-check",
        "--version",
        "local-browser-bridge.exe\" &",
        "local-computer-helper.exe\" &",
    ] {
        assert!(
            !script.contains(forbidden),
            "candidate trust binder must not execute candidate bytes: {forbidden}"
        );
    }
}

#[test]
fn bash_candidate_binder_requires_static_only_release_asset_verification() {
    let script = normalized_source("scripts/fetch-verify-release-candidate.sh");
    let expected_call = concat!(
        "bash \"$SOURCE_ROOT/scripts/verify-release-assets.sh\" \\\n",
        "  \"$VERSION\" \"$PAYLOAD_DIRECTORY\" --static-only >/dev/null"
    );

    assert!(
        script.contains(expected_call),
        "the candidate binder must explicitly select the non-executing release-asset verifier path"
    );
    assert_eq!(
        script.matches("--static-only").count(),
        1,
        "the binder must have one unambiguous static-only policy call"
    );
}

#[test]
fn withdrawn_v01211_candidate_metadata_is_explicitly_non_runtime_evidence() {
    let root = "evidence/v0.12.11/computer/attempts/withdrawn-414dd7f-macos-dual-lane-receipt-gap";
    let readme = fs::read_to_string(format!("{root}/README.md")).unwrap();
    let metadata: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(format!("{root}/candidate-metadata.json")).unwrap(),
    )
    .unwrap();

    assert_eq!(metadata["status"], "withdrawn-before-execution");
    assert_eq!(metadata["reason"], "macos-dual-lane-receipt-gap");
    assert_eq!(metadata["candidateExecuted"], false);
    assert_eq!(metadata["macosStarted"], false);
    assert_eq!(metadata["windowsStarted"], false);
    assert_eq!(metadata["stockChromeStarted"], false);
    assert_eq!(metadata["releaseCreated"], false);
    assert_eq!(metadata["publishJobStepsRun"], 0);
    assert!(readme.contains("must not be retried or reused"));
    assert!(readme.replace('\n', " ").contains("not runtime evidence"));
}

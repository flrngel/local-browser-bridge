use std::fs;

fn normalized_source(path: &str) -> String {
    fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

#[test]
fn macos_release_archive_inventory_is_canonical_and_shared_by_all_producers_and_consumers() {
    let expected = [
        "LICENSE",
        "Local Browser Bridge.app",
        "Local Browser Bridge.app/Contents",
        "Local Browser Bridge.app/Contents/Info.plist",
        "Local Browser Bridge.app/Contents/MacOS",
        "Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop",
        "Local Browser Bridge.app/Contents/_CodeSignature",
        "Local Browser Bridge.app/Contents/_CodeSignature/CodeResources",
        "Local Computer Helper.app",
        "Local Computer Helper.app/Contents",
        "Local Computer Helper.app/Contents/Info.plist",
        "Local Computer Helper.app/Contents/MacOS",
        "Local Computer Helper.app/Contents/MacOS/local-computer-helper",
        "Local Computer Helper.app/Contents/_CodeSignature",
        "Local Computer Helper.app/Contents/_CodeSignature/CodeResources",
        "THIRD_PARTY_LICENSES.txt",
        "local-browser-bridge",
    ];
    let inventory = normalized_source("packaging/macos/release-archive-inventory.txt");
    assert_eq!(inventory, expected.join("\n") + "\n");

    let windows = normalized_source("scripts/verify-windows-release-candidate.ps1");
    assert!(windows.contains(
        "$InventoryPath = Join-Path $SourceRoot \"packaging/macos/release-archive-inventory.txt\""
    ));
    assert!(windows.contains("$ExpectedEntries.Count -ne 17"));
    assert!(
        windows
            .contains("\"Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop\"")
    );
    assert!(windows.contains(
        "[pscustomobject]@{ path = \"Local Browser Bridge.app/Contents/Info.plist\"; label = \"desktop\" }"
    ));

    for producer in [
        normalized_source(".github/workflows/deploy.yml"),
        normalized_source("scripts/deploy.sh"),
    ] {
        assert!(producer.contains("packaging/macos/release-archive-inventory.txt"));
        assert!(producer.contains("sed 's:/$::' | LC_ALL=C sort"));
    }
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
        "[string]$Destination",
        "[string]$TrustedGit",
        "[string]$TrustedGh",
        "[switch]$SelfTest",
        "System32\", \"WindowsPowerShell\", \"v1.0\", \"powershell.exe",
        "Sysnative\", \"WindowsPowerShell\", \"v1.0\", \"powershell.exe",
        "[Microsoft.Win32.RegistryView]::Registry64",
        "[Microsoft.Win32.RegistryKey]::OpenBaseKey",
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
        "-NoLogo -NoProfile -NonInteractive -File",
        "GH_TOKEN must be present before the one-shot Windows trust gate is invoked.",
        "$Info.EnvironmentVariables[\"GH_TOKEN\"] = $CallerGhToken",
        "The clean Windows trust child did not receive GH_TOKEN.",
        "[Environment]::Is64BitProcess",
        "MainModule.FileName",
        "LBB_WINDOWS_TRUST_NONCE",
        "The trust body must run only in its exact self-spawned 64-bit system powershell.exe child.",
        "Windows release-candidate trust wrapper self-test passed.",
        "Self-test cleanup refused an unexpected or linked inventory.",
        "Self-test stops here: no candidate path, network client, token, archive, or",
        "Destination must be a fresh short path",
        "function Assert-PrivateDirectoryAcl([string]$Path)",
        "function New-PrivateDirectory([string]$Path)",
        "$Security.SetOwner($Identity.User)",
        "SetAccessRuleProtection($true, $false)",
        "[IO.FileSystemAclExtensions]::Create(",
        "[IO.Directory]::CreateDirectory($Path, $Security)",
        "New-PrivateDirectory $Root",
        "New-PrivateDirectory $Destination",
        "AreAccessRulesCanonical",
        "Private directory collision self-test failed.",
        "Inherited directory ACL self-test failed.",
        "Fresh destination ACL is not protected and private to the current user.",
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
        "Executing trust wrapper does not match the exact source wrapper blob.",
        "\"attestation\", \"verify\"",
        "--deny-self-hosted-runners\", \"--format\", \"json",
        "$EntryInvocation -cne $CertificateInvocation",
        "$CurrentAttemptCount -ne 1",
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
    let self_test_private_directory = script.find("New-PrivateDirectory $Root").unwrap();
    let live_private_directory = script.rfind("New-PrivateDirectory $Destination").unwrap();
    let token_read = script.find("$InheritedToken =").unwrap();
    let source_clone = script.find("Fixed-origin fresh source clone").unwrap();
    let token_required = script
        .find("The clean Windows trust child did not receive GH_TOKEN.")
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
    assert!(self_test_private_directory < self_test_stop);
    assert!(self_test_stop < live_private_directory);
    assert!(self_test_stop < token_read);
    assert!(token_read < source_clone);
    assert!(token_read < token_required);
    assert!(token_required < source_clone);
    assert!(source_clone < first_api);
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
    assert!(!script.contains("Set-DirectoryAccessControlPortable"));
    assert!(script.contains("$ExpectedArtifactBytes = [int64]$DirectArtifact.size_in_bytes"));
    assert!(
        script.contains("$ExpectedArtifactSha256 = ([string]$DirectArtifact.digest).Substring(7)")
    );
}

#[test]
fn all_candidate_consumers_select_one_exact_attempt_from_valid_same_run_attestations() {
    let bash_paths = [
        "scripts/fetch-verify-release-candidate.sh",
        "scripts/verify-release-acceptance-evidence.sh",
    ];
    let powershell_paths = [
        "scripts/verify-windows-release-candidate.ps1",
        "scripts/test-windows-stock-chrome.ps1",
    ];

    let bash_sources: Vec<String> = bash_paths
        .iter()
        .map(|path| normalized_source(path))
        .collect();
    let bash_filters: Vec<&str> = bash_sources
        .iter()
        .map(|source| {
            source
                .split("# BEGIN EXACT_ATTEMPT_ATTESTATION_FILTER\n")
                .nth(1)
                .unwrap()
                .split("# END EXACT_ATTEMPT_ATTESTATION_FILTER")
                .next()
                .unwrap()
        })
        .collect();
    assert_eq!(
        bash_filters[0], bash_filters[1],
        "both Bash trust gates must execute the same attestation selector"
    );
    for required in [
        "all(.[]; valid_attestation)",
        "$entry_invocation | startswith($same_run_invocation_prefix)",
        "test(\"^[1-9][0-9]*$\")",
        ".verificationResult.statement.predicate.runDetails.metadata.invocationId ==\n        .verificationResult.signature.certificate.runInvocationURI",
        "([.[] | select(\n        .name == $subject_name and .digest.sha256 == $subject_sha256\n      )] | length) == 1",
        "([.[] | select(\n      .verificationResult.statement.predicate.runDetails.metadata.invocationId == $invocation",
        ")] | length) == 1",
    ] {
        assert!(
            bash_filters[0].contains(required),
            "Bash attestation selector is missing `{required}`"
        );
    }
    assert!(!bash_filters[0].contains("test($same_run_invocation_pattern)"));
    for source in &bash_sources {
        for case in [
            "old-only",
            "duplicate current",
            "malformed current",
            "wrong current subject",
        ] {
            assert!(
                source.contains(case),
                "Bash attestation self-test is missing `{case}`"
            );
        }
        assert!(source.contains("verify_exact_attempt_attestation_set"));
    }

    let powershell_sources: Vec<String> = powershell_paths
        .iter()
        .map(|path| normalized_source(path))
        .collect();
    let powershell_selectors: Vec<&str> = powershell_sources
        .iter()
        .map(|source| {
            source
                .split("# BEGIN EXACT_ATTEMPT_ATTESTATION_SELECTOR\n")
                .nth(1)
                .unwrap()
                .split("# END EXACT_ATTEMPT_ATTESTATION_SELECTOR")
                .next()
                .unwrap()
        })
        .collect();
    assert_eq!(
        powershell_selectors[0], powershell_selectors[1],
        "both Windows trust gates must execute the same attestation selector"
    );
    for required in [
        "$Attestations.Count -eq 1 -and\n      $Attestations[0] -is [Array]",
        "$Attestations = [object[]]$Attestations[0]",
        "$ExpectedInvocationUri.StartsWith($SameRunInvocationPrefix, [StringComparison]::Ordinal)",
        "$EntryInvocation.StartsWith($SameRunInvocationPrefix, [StringComparison]::Ordinal)",
        "$Attestation -is [Array]",
        "$Statement.subject -isnot [Array]",
        "$EntryAttemptSuffix -cnotmatch '^[1-9][0-9]*$'",
        "$EntryInvocation -cne $CertificateInvocation",
        "$MatchingSubjects.Count -ne 1",
        "$CurrentAttemptCount -ne 1",
        "malformed, unrelated, or ambiguous statement",
    ] {
        assert!(
            powershell_selectors[0].contains(required),
            "PowerShell attestation selector is missing `{required}`"
        );
    }
    for source in &powershell_sources {
        for case in [
            "old-only",
            "duplicate-current",
            "malformed-current",
            "wrong-current-subject",
            "scalar-current-subject",
            "malformed-current-subject",
            "nested-attestation-array",
        ] {
            assert!(
                source.contains(case),
                "PowerShell attestation self-test is missing `{case}`"
            );
        }
        assert!(source.contains("$Ps51WrappedRoundTrip = New-Object object[] 1"));
        assert!(source.contains("$Ps51WrappedRoundTrip[0] = [object[]]@($Old, $Current)"));
        assert!(source.contains("Invoke-AttestationSelectionSelfTest"));
    }
}

#[test]
fn dedicated_windows_fixture_is_trusted_source_only_not_a_release_asset() {
    let windows_binder = normalized_source("scripts/verify-windows-release-candidate.ps1");
    let bash_binder = normalized_source("scripts/fetch-verify-release-candidate.sh");
    let candidate_workflow = normalized_source(".github/workflows/deploy.yml");
    let ci = normalized_source(".github/workflows/ci.yml");
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

    let assembled_assets = candidate_workflow
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
    assert!(
        candidate_workflow.contains("(cd dist && sha256sum \"${assets[@]}\" > SHA256SUMS.txt)")
    );
    assert!(candidate_workflow.contains("name: release-candidate\n          path: dist/*"));
    assert!(ci.contains(
        "$fixtureExecutableSelfTest = Join-Path $env:RUNNER_TEMP (\"lbb-windows-fixture-\""
    ));
    assert!(!candidate_workflow.contains("$fixtureExecutableSelfTest"));
    assert!(!candidate_workflow.contains("dist/lbb-windows-fixture-"));
    assert!(!candidate_workflow.contains("dist/lbb-windows-computer-use-fixture"));
    assert!(!candidate_workflow.contains("Copy-Item $fixtureExecutableSelfTest"));

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
        "GH_TOKEN must be present before the one-shot Windows trust gate is invoked.",
        "$Info.EnvironmentVariables[\"GH_TOKEN\"] = $CallerGhToken",
        "The clean Windows trust child did not receive GH_TOKEN.",
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
        "Read-Host",
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
fn windows_candidate_playbook_supplies_and_clears_the_noninteractive_token() {
    let playbook = normalized_source("evidence/v0.12.55/browser/README.md");
    for required in [
        "`GH_TOKEN` must already be present when",
        "$trustedGh auth token --hostname github.com",
        "$previousGhToken = [Environment]::GetEnvironmentVariable(\"GH_TOKEN\", \"Process\")",
        "[Environment]::SetEnvironmentVariable(\"GH_TOKEN\", $ghToken.Trim(), \"Process\")",
        "[Environment]::SetEnvironmentVariable(\"GH_TOKEN\", $previousGhToken, \"Process\")",
        "$ghToken = $null",
    ] {
        assert!(
            playbook.contains(required),
            "Windows candidate playbook is missing `{required}`"
        );
    }
    assert!(!playbook.contains("Read-Host"));
}

#[test]
fn bash_and_windows_candidate_binding_schemas_stay_identical() {
    let bash = normalized_source("scripts/fetch-verify-release-candidate.sh");
    let windows = normalized_source("scripts/verify-windows-release-candidate.ps1");
    let fields = [
        "schemaVersion",
        "version",
        "releaseTag",
        "repository",
        "sourceSha",
        "workflowRunId",
        "workflowRunAttempt",
        "workflowEvent",
        "workflowRef",
        "workflowPath",
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

    let bash_binding = bash.rfind("'{schemaVersion:3").unwrap();
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
        "--source-ref \"$WORKFLOW_REF\"",
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

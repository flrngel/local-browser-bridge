use std::fs;
#[cfg(not(target_os = "windows"))]
use std::process::Command;

fn source(path: &str) -> String {
    fs::read_to_string(path).unwrap().replace("\r\n", "\n")
}

fn workflow_job_ids(workflow: &str) -> Vec<&str> {
    let mut in_jobs = false;
    let mut jobs = Vec::new();
    for line in workflow.lines() {
        if line == "jobs:" {
            in_jobs = true;
            continue;
        }
        if !in_jobs {
            continue;
        }
        if !line.is_empty() && !line.starts_with(' ') {
            break;
        }
        if line.starts_with("  ") && !line.starts_with("    ") && line.trim_end().ends_with(':') {
            jobs.push(line.trim().trim_end_matches(':'));
        }
    }
    jobs
}

fn job_section<'a>(workflow: &'a str, job: &str) -> &'a str {
    let start_marker = format!("  {job}:\n");
    let start = workflow
        .find(&start_marker)
        .unwrap_or_else(|| panic!("workflow does not contain job `{job}`"));
    let after = &workflow[start + start_marker.len()..];
    let end = after
        .lines()
        .scan(0usize, |offset, line| {
            let current = *offset;
            *offset += line.len() + 1;
            Some((current, line))
        })
        .find_map(|(offset, line)| {
            (line.starts_with("  ") && !line.starts_with("    ") && line.trim_end().ends_with(':'))
                .then_some(offset)
        })
        .unwrap_or(after.len());
    &after[..end]
}

#[test]
fn extension_package_sources_are_lf_stable_on_every_checkout() {
    let attributes = source(".gitattributes");
    assert!(
        attributes
            .lines()
            .any(|line| line.trim() == "* text=auto eol=lf"),
        ".gitattributes must keep every text source LF-stable for cross-platform contract tests and release packages"
    );
    for pattern in [
        "extension/*.css text eol=lf",
        "extension/*.html text eol=lf",
        "extension/*.js text eol=lf",
        "extension/*.json text eol=lf",
    ] {
        assert!(
            attributes.lines().any(|line| line.trim() == pattern),
            ".gitattributes must contain `{pattern}` so Windows checkouts cannot rewrite packaged extension sources or break source-contract tests"
        );
    }
}

#[test]
fn candidate_fetch_verifier_self_test_follows_syntax_validation_everywhere() {
    let syntax_check = "bash -n scripts/fetch-verify-release-candidate.sh";
    let self_test = "bash scripts/fetch-verify-release-candidate.sh --self-test";

    for path in [
        ".github/workflows/ci.yml",
        ".github/workflows/deploy.yml",
        "scripts/deploy.sh",
    ] {
        let integration = source(path);
        let lines: Vec<_> = integration.lines().collect();
        assert!(
            lines
                .windows(2)
                .any(|pair| pair[0].trim() == syntax_check && pair[1].trim() == self_test),
            "{path} must run the release-candidate verifier self-test immediately after its retained bash syntax check"
        );
    }
}

#[test]
fn release_workflow_and_local_builder_package_both_processes() {
    let workflow = source(".github/workflows/deploy.yml");
    let local = source("scripts/deploy.sh");
    for required in [
        "local-browser-bridge-v${version}-windows-x86_64.exe",
        "local-computer-helper-v${version}-windows-x86_64.exe",
        "local-browser-bridge-v${version}-macos-universal.tar.gz",
        "Local Computer Helper.app/Contents/MacOS/local-computer-helper",
        "THIRD_PARTY_LICENSES.txt",
        "codesign --verify --deep --strict",
        "bash scripts/verify-macos-build-host.sh",
        "bash scripts/verify-macos-artifacts.sh",
    ] {
        assert!(
            workflow.contains(required),
            "workflow is missing {required}"
        );
        assert!(
            local.contains(required),
            "local builder is missing {required}"
        );
    }
    assert!(workflow.contains("local-browser-bridge-extension-v${VERIFIED_VERSION}.zip"));
    assert!(local.contains("local-browser-bridge-extension-v${version}.zip"));
    assert!(workflow.contains("bash scripts/verify-macos-artifacts.sh"));
    assert!(source("scripts/verify-macos-artifacts.sh").contains("--licenses"));
    assert!(workflow.contains("cargo build --locked --release --bins"));
    assert!(local.contains("cargo xwin build --locked --release --bins"));
    assert!(local.contains("release_stage=\"$(mktemp -d)\""));
    assert!(local.contains("validation_stage=\"$(mktemp -d)\""));
    assert!(
        local.contains("bash scripts/verify-macos-app-share-handoff-self-test.sh \"$version\"")
    );
    let handoff_self_test = source("scripts/verify-macos-app-share-handoff-self-test.sh");
    assert!(handoff_self_test.contains("scratch=\"$(mktemp -d"));
    assert!(!handoff_self_test.contains("release_stage"));
    assert!(
        source(".github/workflows/ci.yml")
            .contains("verify-macos-app-share-handoff-self-test.sh 0.12.41 --historical-source")
    );
    assert!(
        source(".github/workflows/ci.yml")
            .contains("verify-macos-app-share-handoff-self-test.sh 0.12.66\n")
    );
    assert!(!source(".github/workflows/deploy.yml").contains("--historical-source"));
    assert!(!local.contains("--historical-source"));
    assert!(
        local.contains("bash scripts/verify-release-assets.sh \"$version\" \"$release_stage\"")
    );
    assert!(local.contains("cp \"$release_stage/$asset\" \"$publish_stage/$asset\""));
}

#[test]
fn local_deploy_atomically_replaces_only_generated_release_assets_with_rollback() {
    let local = source("scripts/deploy.sh");

    for required in [
        "is_recognized_generated_release_asset() {",
        "^local-browser-bridge-v[0-9]+\\.[0-9]+\\.[0-9]+-windows-x86_64\\.exe$",
        "^local-computer-helper-v[0-9]+\\.[0-9]+\\.[0-9]+-windows-x86_64\\.exe$",
        "^local-browser-bridge-v[0-9]+\\.[0-9]+\\.[0-9]+-macos-universal\\.tar\\.gz$",
        "^local-browser-bridge-extension-v[0-9]+\\.[0-9]+\\.[0-9]+\\.zip$",
        "[[ \"$name\" == \"SHA256SUMS.txt\" ]]",
        "Refusing to replace a linked dist path",
        "Refusing to replace dist because it contains a linked or non-file entry",
        "Refusing to replace dist because it contains an unrecognized entry",
        "publish_stage=\"$(mktemp -d \"$project_root/.dist-publish.XXXXXX\")\"",
        "dist_rollback_parent=\"$(mktemp -d \"$project_root/.dist-rollback.XXXXXX\")\"",
        "mv \"$dist_dir\" \"$dist_rollback_path\"",
        "mv \"$publish_stage\" \"$dist_dir\"",
        "restore_dist_rollback || true",
        "quarantine_unverified_dist() {",
        "failed_publish_parent=\"$(mktemp -d \"$project_root/.dist-failed.XXXXXX\")\"",
        "mv \"$dist_dir\" \"$failed_publish_path\"",
        "Preserved the unverified dist replacement at",
        "trap cleanup EXIT",
        "trap 'exit 130' INT",
        "trap 'exit 143' TERM",
    ] {
        assert!(
            local.contains(required),
            "local deploy atomic-publish contract is missing: {required}"
        );
    }

    assert_eq!(
        local
            .matches("validate_replaceable_dist \"$dist_dir\"")
            .count(),
        2,
        "existing dist must be checked before the build and again immediately before replacement"
    );
    assert!(
        !local.contains("cp \"$release_stage/$asset\" \"dist/$asset\""),
        "release assets must never be copied individually into the live dist directory"
    );

    let build_stage_verified = local
        .find("bash scripts/verify-release-assets.sh \"$version\" \"$release_stage\"")
        .unwrap();
    let publish_stage_created = local
        .find("publish_stage=\"$(mktemp -d \"$project_root/.dist-publish.XXXXXX\")\"")
        .unwrap();
    let publish_stage_verified = local
        .find("bash scripts/verify-release-assets.sh \"$version\" \"$publish_stage\"")
        .unwrap();
    let final_existing_dist_check = local
        .rfind("validate_replaceable_dist \"$dist_dir\"")
        .unwrap();
    let old_dist_moved = local
        .find("mv \"$dist_dir\" \"$dist_rollback_path\"")
        .unwrap();
    let rollback_revalidated = local
        .find("validate_replaceable_dist \"$dist_rollback_path\"")
        .unwrap();
    let replacement_installed = local.find("mv \"$publish_stage\" \"$dist_dir\"").unwrap();
    let installed_dist_verified = local
        .find("bash scripts/verify-release-assets.sh \"$version\" \"$dist_dir\"")
        .unwrap();
    let installed_dist_quarantined_on_failure = local[replacement_installed..]
        .find("quarantine_unverified_dist || true")
        .map(|offset| replacement_installed + offset)
        .unwrap();
    let rollback_restored_after_quarantine = local[installed_dist_quarantined_on_failure..]
        .find("restore_dist_rollback || true")
        .map(|offset| installed_dist_quarantined_on_failure + offset)
        .unwrap();
    let installed_dist_committed = local.rfind("dist_publish_verified=1").unwrap();
    let rollback_disarmed = local.rfind("dist_replacement_pending=0").unwrap();
    let rollback_removed = local.find("rm -rf \"$dist_rollback_parent\"").unwrap();

    assert!(build_stage_verified < publish_stage_created);
    assert!(publish_stage_created < publish_stage_verified);
    assert!(publish_stage_verified < final_existing_dist_check);
    assert!(final_existing_dist_check < old_dist_moved);
    assert!(old_dist_moved < rollback_revalidated);
    assert!(rollback_revalidated < replacement_installed);
    assert!(replacement_installed < installed_dist_verified);
    assert!(installed_dist_verified < installed_dist_quarantined_on_failure);
    assert!(installed_dist_quarantined_on_failure < rollback_restored_after_quarantine);
    assert!(rollback_restored_after_quarantine < installed_dist_committed);
    assert!(installed_dist_committed < rollback_disarmed);
    assert!(rollback_disarmed < rollback_removed);

    let cleanup = local
        .split("cleanup() {")
        .nth(1)
        .unwrap()
        .split("\n}\n\ntrap cleanup EXIT")
        .next()
        .unwrap();
    let cleanup_quarantine = cleanup.find("quarantine_unverified_dist").unwrap();
    let cleanup_restore = cleanup.find("restore_dist_rollback").unwrap();
    assert!(
        cleanup_quarantine < cleanup_restore,
        "EXIT and signal cleanup must quarantine an installed-but-unverified replacement before restoring the previous dist"
    );
}

#[test]
fn windows_ci_validates_the_complete_browser_evidence_toolchain_before_candidate_builds() {
    for path in [".github/workflows/ci.yml"] {
        let workflow = source(path);
        for script in [
            "scripts/browser-evidence-candidate.ps1",
            "scripts/record-computer-helper-chain.ps1",
            "scripts/run-windows-computer-use-acceptance.ps1",
            "scripts/sanitize-browser-evidence-screenshot.ps1",
            "scripts/test-windows-browser-api.ps1",
            "scripts/test-windows-computer-use.ps1",
            "scripts/test-windows-stock-chrome.ps1",
            "scripts/verify-windows-artifacts.ps1",
            "scripts/verify-windows-release-candidate.ps1",
            "scripts/wait-windows-foreground-arm-handoff.ps1",
            "scripts/write-browser-evidence-record.ps1",
            "scripts/write-stock-chrome-operator-response.ps1",
            "tests/fixtures/windows/WindowsComputerUseFixture.ps1",
        ] {
            assert!(
                workflow.contains(&format!("\"{script}\"")),
                "Windows validation in {path} does not parse {script}"
            );
        }
        for invocation in [
            "./scripts/browser-evidence-candidate.ps1 -Mode SelfTest",
            "./scripts/record-computer-helper-chain.ps1 -Mode SelfTest",
            "./scripts/run-windows-computer-use-acceptance.ps1 -Mode SelfTest",
            "./scripts/sanitize-browser-evidence-screenshot.ps1 -Mode SelfTest",
            "./scripts/test-windows-browser-api.ps1 -SelfTest",
            "./scripts/verify-windows-artifacts.ps1 -Version 0.0.0 -SelfTest",
            "./scripts/wait-windows-foreground-arm-handoff.ps1 -Mode SelfTest",
            "./tests/fixtures/windows/WindowsComputerUseFixture.ps1 -SelfTest",
            "./scripts/write-browser-evidence-record.ps1 -Mode SelfTest",
            "./scripts/write-stock-chrome-operator-response.ps1 -Mode SelfTest",
        ] {
            assert!(
                workflow.contains(invocation),
                "Windows validation in {path} does not run {invocation}"
            );
        }
        for required in [
            "$windowsPowerShell = [IO.Path]::GetFullPath([IO.Path]::Combine(",
            "$env:SystemRoot, \"System32\", \"WindowsPowerShell\", \"v1.0\", \"powershell.exe\"",
            "$PSVersionTable.PSVersion.Major",
            "$PSVersionTable.PSVersion.Minor",
            "$PSVersionTable.PSEdition",
            "$ps51Identity[0] -cne \"5.1|Desktop\"",
            "The exact system PowerShell self-test host is not Windows PowerShell 5.1 Desktop.",
            "$watcherPathForPs51 = (Resolve-Path ./scripts/wait-windows-foreground-arm-handoff.ps1).Path",
            "$watcherScript = [ScriptBlock]::Create([IO.File]::ReadAllText([IO.Path]::GetFullPath($env:LBB_PS51_WATCHER_SELF_TEST)))",
            "& $watcherScript -Mode SelfTest",
            "$watcherCallOperatorOutput.Count -ne 1",
            "Windows PowerShell 5.1 foreground-arm call-operator self-test failed.",
            "Remove-Item Env:\\LBB_PS51_WATCHER_SELF_TEST -ErrorAction SilentlyContinue",
        ] {
            assert!(
                workflow.contains(required),
                "Windows PowerShell 5.1 validation in {path} is missing {required}"
            );
        }
        let identity_gate = workflow
            .find("$ps51Identity[0] -cne \"5.1|Desktop\"")
            .unwrap();
        let first_self_test = workflow
            .find("./scripts/browser-evidence-candidate.ps1 -Mode SelfTest")
            .unwrap();
        assert!(
            identity_gate < first_self_test,
            "Windows PowerShell identity must be proven before any PowerShell self-test in {path}"
        );
        assert!(
            !workflow.contains("\n          powershell.exe -NoLogo"),
            "Windows validation in {path} must not resolve an ambient powershell.exe"
        );
        for invocation in [
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/browser-evidence-candidate.ps1 -Mode SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/record-computer-helper-chain.ps1 -Mode SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/run-windows-computer-use-acceptance.ps1 -Mode SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/sanitize-browser-evidence-screenshot.ps1 -Mode SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/test-windows-browser-api.ps1 -SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/write-browser-evidence-record.ps1 -Mode SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/write-stock-chrome-operator-response.ps1 -Mode SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/verify-windows-artifacts.ps1 -Version 0.0.0 -SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/test-windows-stock-chrome.ps1 -SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/verify-windows-release-candidate.ps1 -SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/wait-windows-foreground-arm-handoff.ps1 -Mode SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./tests/fixtures/windows/WindowsComputerUseFixture.ps1 -SelfTest",
        ] {
            assert!(
                workflow.contains(invocation),
                "Windows validation in {path} does not run through the exact system PowerShell: {invocation}"
            );
        }
    }

    let candidate = source(".github/workflows/deploy.yml");
    assert!(candidate.contains("./scripts/verify-windows-artifacts.ps1 -Version $version"));
    assert!(candidate.contains("needs: verify"));
    assert!(candidate.contains("ref: ${{ needs.verify.outputs.source_sha }}"));
    assert!(
        !candidate.contains("run-windows-computer-use-acceptance.ps1 -Mode SelfTest"),
        "the candidate workflow must reuse the exact reviewed CI result instead of paying to repeat the native acceptance coordinator"
    );
}

#[test]
fn windows_ci_gates_the_acceptance_coordinator_under_exact_ps51() {
    let invocation = "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/run-windows-computer-use-acceptance.ps1 -Mode SelfTest";
    let success = "Windows computer-use acceptance coordinator self-test passed.";
    for path in [".github/workflows/ci.yml"] {
        let workflow = source(path);
        for required in [
            "\"scripts/run-windows-computer-use-acceptance.ps1\"",
            "$coordinatorPathForPs51 = (Resolve-Path ./scripts/run-windows-computer-use-acceptance.ps1).Path",
            "$previousCoordinatorPathForPs51 = $env:LBB_PS51_COORDINATOR_PARSE",
            "[Management.Automation.Language.Parser]::ParseFile([IO.Path]::GetFullPath($env:LBB_PS51_COORDINATOR_PARSE), [ref]$tokens, [ref]$errors)",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -Command $coordinatorParserCommand",
            "Windows PowerShell 5.1 computer-use acceptance coordinator parser failed.",
            "Remove-Item Env:\\LBB_PS51_COORDINATOR_PARSE -ErrorAction SilentlyContinue",
            invocation,
            "$coordinatorSelfTestOutput.Count -ne 1",
            "$coordinatorSelfTestOutput[0] -cne \"Windows computer-use acceptance coordinator self-test passed.\"",
            "Windows PowerShell 5.1 computer-use acceptance coordinator self-test failed.",
        ] {
            assert!(
                workflow.contains(required),
                "Windows coordinator CI gate in {path} is missing: {required}"
            );
        }
        assert_eq!(
            workflow.matches(invocation).count(),
            1,
            "{path} must execute exactly one coordinator self-test through exact system Windows PowerShell 5.1"
        );
        assert_eq!(
            workflow.matches(success).count(),
            1,
            "{path} must require the exact coordinator success message once"
        );
        assert!(
            !workflow.contains("test-windows-computer-use.ps1 -SelfTest"),
            "{path} must not bypass the topology-aware coordinator with a direct Job-sensitive runner self-test"
        );
        let identity_gate = workflow
            .find("$ps51Identity[0] -cne \"5.1|Desktop\"")
            .unwrap();
        let coordinator_parser = workflow
            .find("& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -Command $coordinatorParserCommand")
            .unwrap();
        let coordinator_gate = workflow.find(invocation).unwrap();
        assert!(
            identity_gate < coordinator_parser && coordinator_parser < coordinator_gate,
            "{path} must prove exact system Windows PowerShell 5.1, parse the coordinator there, and only then run its self-test"
        );
    }
}

#[test]
fn current_source_is_unblocked_and_package_versions_are_aligned() {
    match fs::symlink_metadata("RELEASE_BLOCKED") {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("could not inspect the release-blocker path: {error}"),
        Ok(_) => {
            panic!("the reviewed source must not retain a release-blocker file or symlink")
        }
    }

    for (path, required) in [
        ("Cargo.toml", "version = \"0.12.66\""),
        (
            "Cargo.lock",
            "name = \"local-browser-bridge\"\nversion = \"0.12.66\"",
        ),
        ("extension/manifest.json", "\"version\": \"0.12.66\""),
        ("extension/lib.js", "export const VERSION = \"0.12.66\";"),
        (
            "scripts/run-windows-computer-use-acceptance.ps1",
            "$script:ProductVersion = \"0.12.66\"",
        ),
        (
            "scripts/finalize-macos-acceptance.mjs",
            "const PRODUCT_VERSION = \"0.12.66\";",
        ),
        (
            "scripts/record-computer-helper-chain.ps1",
            "$script:Version = \"0.12.66\"",
        ),
        (
            "scripts/test-windows-stock-chrome.ps1",
            "$Version = \"0.12.66\"",
        ),
        (
            "scripts/verify-release-acceptance-evidence.sh",
            "readonly EVIDENCE_PRODUCT_VERSION=\"0.12.66\"",
        ),
        (
            "scripts/verify-windows-release-candidate.ps1",
            "$ProductVersion = \"0.12.66\"",
        ),
        (
            "scripts/write-browser-evidence-record.ps1",
            "$script:OperatorV2Version = \"0.12.66\"",
        ),
        (
            "scripts/write-stock-chrome-operator-response.ps1",
            "$script:Version = \"0.12.66\"",
        ),
    ] {
        assert!(
            source(path).contains(required),
            "source or retained release-evidence version alignment is missing from {path}: {required}"
        );
    }

    assert!(
        source("evidence/v0.12.66/computer/AppShareHandoff.swift")
            .contains("private let productVersion = \"0.12.66\"")
    );

    assert!(std::path::Path::new("evidence/v0.12.66/browser").is_dir());
    assert!(std::path::Path::new("evidence/v0.12.66/computer").is_dir());
    assert!(std::path::Path::new("evidence/v0.12.41/browser").is_dir());
    assert!(std::path::Path::new("evidence/v0.12.41/computer").is_dir());
}

#[test]
fn release_paths_retain_generic_fail_closed_blocker_enforcement() {
    let candidate = source(".github/workflows/deploy.yml");
    let publication = source(".github/workflows/publish.yml");
    let local = source("scripts/deploy.sh");

    let workflow_condition = "[[ -e RELEASE_BLOCKED || -L RELEASE_BLOCKED ]]";
    assert_eq!(candidate.matches(workflow_condition).count(), 1);
    assert_eq!(
        publication.matches(workflow_condition).count(),
        1,
        "publication must recheck the source blocker independently of candidate creation"
    );
    assert!(candidate.contains("Release is blocked by RELEASE_BLOCKED."));
    assert!(publication.contains("Release is blocked by RELEASE_BLOCKED."));
    let workflow_gate = candidate.find(workflow_condition).unwrap();
    let workflow_build = candidate.find("bash scripts/package-extension.sh").unwrap();
    assert!(workflow_gate < workflow_build);
    assert!(!candidate.contains("continue-on-error:"));
    assert!(!publication.contains("continue-on-error:"));
    for dependency in [
        "windows:\n    name: Build Windows x86_64\n    needs: verify",
        "macos:\n    name: Build macOS universal\n    needs: verify",
        "assemble:\n    name: Assemble frozen release candidate\n    needs: [verify, windows, macos]",
    ] {
        assert!(
            candidate.contains(dependency),
            "candidate dependency closure is missing: {dependency}"
        );
    }
    assert!(
        publication
            .contains("release:\n    name: Publish exact accepted release\n    needs: preflight")
    );
    let publication_gate = publication.find(workflow_condition).unwrap();
    let protected_environment = publication
        .find("environment:\n      name: release")
        .unwrap();
    assert!(publication_gate < protected_environment);

    let local_condition = "[[ -e \"$release_blocker\" || -L \"$release_blocker\" ]]";
    assert_eq!(local.matches(local_condition).count(), 1);
    assert!(
        local.contains("Release is blocked by RELEASE_BLOCKED; resolve its recorded source gate")
    );
    let local_gate = local.find(local_condition).unwrap();
    for forbidden_before_gate in [
        "version=\"$(bash scripts/audit-versions.sh)\"",
        "release_stage=\"$(mktemp -d",
        "cargo build --locked --release",
        "validate_replaceable_dist \"$dist_dir\"",
    ] {
        assert!(
            local_gate < local.find(forbidden_before_gate).unwrap(),
            "local release blocker must precede: {forbidden_before_gate}"
        );
    }
}

#[test]
fn windows_ci_compiles_executes_and_self_cleans_the_dedicated_fixture() {
    for path in [".github/workflows/ci.yml"] {
        let workflow = source(path);
        for required in [
            "$fixtureExecutableSelfTest = Join-Path $env:RUNNER_TEMP (\"lbb-windows-fixture-\" + [Guid]::NewGuid().ToString(\"N\") + \".exe\")",
            "$fixtureSourcePath = (Resolve-Path ./tests/fixtures/windows/WindowsComputerUseFixture.ps1).Path",
            "$fixtureSourceStream = [IO.File]::OpenRead($fixtureSourcePath)",
            "$fixtureSourceHasher = [Security.Cryptography.SHA256]::Create()",
            "$fixtureSourceSha256 = (($fixtureSourceHasher.ComputeHash($fixtureSourceStream) | ForEach-Object { $_.ToString(\"x2\") }) -join '')",
            "$fixtureSourceHasher.Dispose()",
            "$fixtureSourceStream.Dispose()",
            "$fixtureBuildOutput = @(& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File $fixtureSourcePath -BuildExecutablePath $fixtureExecutableSelfTest -ExpectedSourceSha256 $fixtureSourceSha256)",
            "$fixtureBuildOutput.Count -ne 1",
            "$fixtureBuildOutput[0] -cne \"Windows computer-use fixture executable built.\"",
            "throw \"Windows PowerShell 5.1 dedicated fixture build failed.\"",
            "& $fixtureExecutableSelfTest --self-test",
            "throw \"Dedicated Windows fixture executable self-test failed.\"",
            "$fixtureCleanupDeadline = [DateTime]::UtcNow.AddSeconds(10)",
            "if ([IO.Directory]::Exists($fixtureExecutableSelfTest))",
            "throw \"The dedicated Windows fixture executable path was replaced by a directory.\"",
            "$fixtureExecutableAttributes = [IO.File]::GetAttributes($fixtureExecutableSelfTest)",
            "($fixtureExecutableAttributes -band [IO.FileAttributes]::ReparsePoint) -ne 0",
            "[IO.File]::SetAttributes($fixtureExecutableSelfTest, [IO.FileAttributes]::Normal)",
            "[IO.File]::Delete($fixtureExecutableSelfTest)",
            "Start-Sleep -Milliseconds 100",
            "[DateTime]::UtcNow -lt $fixtureCleanupDeadline",
            "The dedicated Windows fixture executable self-test artifact remained after bounded cleanup.",
        ] {
            assert!(
                workflow.contains(required),
                "dedicated fixture CI gate in {path} is missing: {required}"
            );
        }

        let ps_self_test = workflow
            .find("& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./tests/fixtures/windows/WindowsComputerUseFixture.ps1 -SelfTest")
            .unwrap();
        let temp_executable = workflow
            .find("$fixtureExecutableSelfTest = Join-Path $env:RUNNER_TEMP")
            .unwrap();
        let source_hash = workflow
            .find("$fixtureSourceSha256 = (($fixtureSourceHasher.ComputeHash")
            .unwrap();
        let compile = workflow
            .find("$fixtureBuildOutput = @(& $windowsPowerShell")
            .unwrap();
        let execute = workflow
            .find("& $fixtureExecutableSelfTest --self-test")
            .unwrap();
        let cleanup = workflow[execute..]
            .find("finally {")
            .map(|offset| execute + offset)
            .unwrap();
        let remove = workflow
            .find("[IO.File]::Delete($fixtureExecutableSelfTest)")
            .unwrap();
        let refuse_remnant = workflow
            .find("The dedicated Windows fixture executable self-test artifact remained after bounded cleanup.")
            .unwrap();
        assert!(ps_self_test < temp_executable);
        assert!(temp_executable < source_hash);
        assert!(source_hash < compile);
        assert!(compile < execute);
        assert!(execute < cleanup);
        assert!(cleanup < remove);
        assert!(remove < refuse_remnant);

        let dedicated_fixture_gate = &workflow[temp_executable..refuse_remnant];
        assert!(
            !dedicated_fixture_gate.contains("dist/"),
            "the CI-only dedicated fixture executable in {path} must never enter dist"
        );
        assert!(
            !dedicated_fixture_gate.contains("actions/upload-artifact"),
            "the CI-only dedicated fixture executable in {path} must never be uploaded"
        );
    }
}

#[test]
fn ci_runs_every_browser_evidence_self_test_under_windows_powershell_51() {
    let workflow = source(".github/workflows/ci.yml");
    for invocation in [
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/browser-evidence-candidate.ps1 -Mode SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/record-computer-helper-chain.ps1 -Mode SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/run-windows-computer-use-acceptance.ps1 -Mode SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/sanitize-browser-evidence-screenshot.ps1 -Mode SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/test-windows-browser-api.ps1 -SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/write-browser-evidence-record.ps1 -Mode SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/write-stock-chrome-operator-response.ps1 -Mode SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/verify-windows-artifacts.ps1 -Version 0.0.0 -SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/test-windows-stock-chrome.ps1 -SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/verify-windows-release-candidate.ps1 -SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/wait-windows-foreground-arm-handoff.ps1 -Mode SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./tests/fixtures/windows/WindowsComputerUseFixture.ps1 -SelfTest",
    ] {
        assert!(
            workflow.contains(invocation),
            "CI validation does not run under Windows PowerShell 5.1: {invocation}"
        );
    }
}

#[test]
fn windows_release_tooling_hashes_without_module_discovery() {
    for path in [
        ".github/workflows/ci.yml",
        ".github/workflows/deploy.yml",
        "scripts/browser-evidence-candidate.ps1",
        "scripts/record-computer-helper-chain.ps1",
        "scripts/run-windows-computer-use-acceptance.ps1",
        "scripts/sanitize-browser-evidence-screenshot.ps1",
        "scripts/test-windows-browser-api.ps1",
        "scripts/test-windows-computer-use.ps1",
        "scripts/test-windows-stock-chrome.ps1",
        "scripts/verify-windows-artifacts.ps1",
        "scripts/verify-windows-release-candidate.ps1",
        "scripts/wait-windows-foreground-arm-handoff.ps1",
        "scripts/write-browser-evidence-record.ps1",
        "scripts/write-stock-chrome-operator-response.ps1",
        "tests/fixtures/windows/WindowsComputerUseFixture.ps1",
    ] {
        let tooling = source(path);
        assert!(
            !tooling.contains("Get-FileHash"),
            "{path} must hash through the .NET cryptography API so a restricted PSModulePath cannot disable a release or acceptance gate"
        );
    }
}

#[test]
fn macos_app_share_handoff_is_release_gated_and_pointer_watcher_is_adversarial_only() {
    let watcher = source("scripts/wait-macos-app-share-concurrency-handoff.mjs");
    let adversarial_watcher = source("scripts/wait-macos-pointer-concurrency-handoff.mjs");
    let producer = source("evidence/v0.12.66/computer/helper-evidence-rig.mjs");
    let playbook = source("evidence/v0.12.66/computer/README.md");
    let finalizer = source("scripts/finalize-macos-acceptance.mjs");
    let verifier = source("scripts/verify-release-acceptance-evidence.sh");
    let handoff_self_test = source("scripts/verify-macos-app-share-handoff-self-test.sh");
    let ci = source(".github/workflows/ci.yml");
    let candidate = source(".github/workflows/deploy.yml");
    let local = source("scripts/deploy.sh");

    for integration in [&ci, &local] {
        assert!(
            integration
                .contains("node --check scripts/wait-macos-app-share-concurrency-handoff.mjs"),
            "CI/local validation does not syntax-check the exact-app-share watcher"
        );
        assert!(integration.contains(
            "node scripts/wait-macos-app-share-concurrency-handoff.mjs --mode self-test"
        ));
    }
    assert!(
        ci.matches("node --check scripts/wait-macos-app-share-concurrency-handoff.mjs")
            .count()
            >= 2
    );
    assert!(
        local
            .matches("node --check scripts/wait-macos-app-share-concurrency-handoff.mjs")
            .count()
            >= 1
    );

    assert!(ci.contains("Validate the optional adversarial macOS pointer handoff watcher"));
    assert_eq!(
        ci.matches("node --check scripts/wait-macos-pointer-concurrency-handoff.mjs")
            .count(),
        1,
        "the legacy pointer watcher must remain optional adversarial CI coverage"
    );
    for release_path in [&candidate, &local] {
        assert!(
            !release_path.contains("wait-macos-pointer-concurrency-handoff.mjs"),
            "the legacy pointer watcher must not gate or satisfy release"
        );
    }
    assert!(adversarial_watcher.contains("const PRODUCT_VERSION = \"0.12.66\";"));
    assert!(
        adversarial_watcher.contains("macOS pointer-concurrency handoff watcher self-test passed.")
    );
    for acceptance_gate in [&producer, &finalizer, &verifier] {
        for forbidden_legacy_receipt in [
            "macos-pointer-concurrency-handoff-request.json",
            "macos-pointer-concurrency-handoff-complete.json",
            "clickFreeMotionObserved",
            "productBoundaryContaminated",
            "independentBoundaryContaminated",
        ] {
            assert!(
                !acceptance_gate.contains(forbidden_legacy_receipt),
                "legacy pointer evidence can still satisfy release through {forbidden_legacy_receipt}"
            );
        }
    }
    for aggregate_contract in [
        "const PRODUCT_VERSION = \"0.12.66\";",
        "const RESULT_SCHEMA_VERSION = 9;",
        "const AGGREGATE_SCHEMA_VERSION = 3;",
        "const REQUEST_MARKER = \"operator/macos-app-share-concurrency-handoff-request.json\";",
        "const START_MARKER = \"operator/macos-app-share-concurrency-handoff-start.json\";",
        "const COMPLETE_MARKER = \"operator/macos-app-share-concurrency-handoff-complete.json\";",
        "aggregate.aggregateChecks.inventoryFileCount !== 19",
        "lane === \"deliberate-concurrency\"",
        "deliberateConcurrency:",
    ] {
        assert!(
            finalizer.contains(aggregate_contract),
            "macOS finalizer is missing {aggregate_contract}"
        );
    }

    for required in [
        "const PRODUCT_VERSION = \"0.12.66\";",
        "const SCHEMA_VERSION = 2;",
        "const OPERATOR_DIRECTORY = \"operator\";",
        "const QUIET_SEAT_MAXIMUM_WAIT_MS = 30 * 60_000;",
        "const PRODUCER_PRE_REQUEST_WORK_BUDGET_MS = 30 * 60_000;",
        "const REQUEST_PUBLICATION_MAXIMUM_WAIT_MS =",
        "const EVIDENCE_DIRECTORY_WAIT_TIMEOUT_MS = 60_000;",
        "const MAX_ACTION_TO_COMPLETE_MS = 18_000;",
        "createdAt: new Date(nowMs + 12_100).toISOString(),",
        "productActionCompletedAt: new Date(nowMs + 12_000).toISOString(),",
        "macos-app-share-concurrency-handoff-request.json",
        "macos-app-share-concurrency-handoff-start.json",
        "macos-app-share-concurrency-handoff-complete.json",
        "dev.flrngel.local-browser-bridge.acceptance.app-share",
        "LBB macOS Acceptance App Share",
        "START APP-SHARE CHECK",
        "lbb-app-share-start",
        "--mode watch --evidence-dir <absolute-path> --runner-pid <pid>",
        "open(path, constants.O_RDONLY | constants.O_NOFOLLOW)",
        "stats.mode & 0o077n",
        "stats.uid !== BigInt(process.getuid())",
        "process.kill(pid, 0)",
        "the request marker disappeared or changed after notification.",
        "the bound request/start chain disappeared or changed before completion.",
        "timed out waiting for the exact-app-share start receipt.",
        "timed out waiting for the bound macOS app-share completion receipt.",
        "ACTION REQUIRED: In the exact app share for",
        "START RECEIVED: The bound button action was recorded.",
        "COMPLETE: Exact-app-share orchestration and the quiet shared-seat product boundary",
        "macOS app-share-concurrency handoff watcher self-test passed.",
    ] {
        assert!(
            watcher.contains(required),
            "macOS app-share handoff watcher is missing {required}"
        );
    }

    for shared_contract in [
        "macos-app-share-concurrency-handoff-request.json",
        "macos-app-share-concurrency-handoff-start.json",
        "macos-app-share-concurrency-handoff-complete.json",
        "macos-app-share-concurrency-handoff-request",
        "macos-app-share-concurrency-handoff-start",
        "macos-app-share-concurrency-handoff-complete",
        "dev.flrngel.local-browser-bridge.acceptance.app-share",
        "LBB macOS Acceptance App Share",
        "START APP-SHARE CHECK",
        "requestSha256",
        "startReceiptSha256",
    ] {
        assert!(watcher.contains(shared_contract));
        assert!(
            producer.contains(shared_contract),
            "macOS app-share producer and watcher disagree on {shared_contract}"
        );
    }
    assert!(producer.contains("const APP_SHARE_HANDOFF_MARKER_SCHEMA = 2;"));
    for shared_timing_contract in [
        "const QUIET_SEAT_MAXIMUM_WAIT_MS = 30 * 60_000;",
        "const PRODUCER_PRE_REQUEST_WORK_BUDGET_MS = 30 * 60_000;",
        "const REQUEST_PUBLICATION_MAXIMUM_WAIT_MS =",
        "QUIET_SEAT_MAXIMUM_WAIT_MS + PRODUCER_PRE_REQUEST_WORK_BUDGET_MS;",
    ] {
        assert!(watcher.contains(shared_timing_contract));
        assert!(
            producer.contains(shared_timing_contract),
            "macOS app-share producer and watcher disagree on {shared_timing_contract}"
        );
    }
    assert!(producer.contains("const APP_SHARE_HANDOFF_COMPLETION_GRACE_MS = 18_000;"));
    assert!(
        producer
            .contains("productCompleted - productStarted <= APP_SHARE_HANDOFF_COMPLETION_GRACE_MS")
    );
    assert!(producer.contains("created - startCreated <= APP_SHARE_HANDOFF_COMPLETION_GRACE_MS"));
    assert!(finalizer.contains("const MAX_ACTION_TO_COMPLETE_MS = 18_000;"));
    for producer_deadline_contract in [
        "outputReservationStartedAtMilliseconds + REQUEST_PUBLICATION_MAXIMUM_WAIT_MS",
        "remainingRequestPublicationTime(",
        "pointerHandoffRequestPublicationDeadlineMilliseconds",
        "request-publication absolute deadline self-test failed",
    ] {
        assert!(
            producer.contains(producer_deadline_contract),
            "macOS app-share producer is missing {producer_deadline_contract}"
        );
    }
    assert!(producer.contains("const operatorDirectory = join(outputDir, \"operator\");"));
    assert!(watcher.contains("join(evidenceDir, OPERATOR_DIRECTORY)"));

    for playbook_boundary in [
        "never concatenate the phases into one script",
        "### Phase 1: bind the candidate, run quiet, then stop for review",
        "### Phase 2: require quiet review, run deliberate, then stop for review",
        "### Phase 3: require both reviews and finalize create-once",
        "SOURCE_ROOT=\"$(cd \"$SOURCE_ROOT\" && pwd -P)\"",
        "PRIVATE_PARENT=\"$(cd \"$PRIVATE_PARENT\" && pwd -P)\"",
        "IFS= read -r QUIET_REVIEWED_RESULT_SHA256",
        "IFS= read -r DELIBERATE_REVIEWED_RESULT_SHA256",
        "QUIET_EXPECTED_INVENTORY=",
        "DELIBERATE_EXPECTED_INVENTORY=",
        "find . -mindepth 1 -print",
        "QUIET_REVIEW_MANIFEST=",
        "DELIBERATE_REVIEW_MANIFEST=",
        "jq -er '.screenshots[] | \"\\(.sha256)  \\(.file)\"' helper-results.json",
        ".status == \"passed-release-candidate\"",
        ".schemaVersion == 3",
        ".aggregateChecks.passingResultSchemaVersion == 9",
        ".aggregateChecks.inventoryFileCount == 19",
        "macos-app-share-concurrency-handoff-start.json",
        ".appShareHandoff.startReceiptAcknowledged == true",
        ".appShareHandoff.completePublicationAcknowledged == true",
        ".appShareHandoff.productBoundaryQuiet == true",
        ".appShareHandoff.independentBoundaryQuiet == true",
        "MACOS_ACCEPTANCE_SHA256=",
        "complete Phase 1 visual review first",
        "complete Phase 2 visual review first",
        "assert_quiet_readiness_record()",
        "QUIET_READINESS_JSON=",
        "DELIBERATE_READINESS_JSON=",
        "--quiet-readiness",
        ".kind == \"macos-quiet-seat-readiness\"",
        ".status == \"ready\"",
        ".acceptanceEvidence == false",
        ".candidateInvocations == 0",
        ".requiredStableMilliseconds == 30000",
        ".maximumWaitMilliseconds == 1800000",
        ".sampleIntervalMilliseconds == 500",
        ".requiredStableTransitions == 60",
        "(.stableDurationMilliseconds | type == \"number\" and . == floor",
        "(.observedSamples | type == \"number\" and . == floor",
        "(.stableTransitions | type == \"number\" and . == floor",
        "(.resetCount | type == \"number\" and . == floor",
        "all($counts[]; type == \"number\" and . == floor",
        ".resetCauseCounts[.lastResetCause] >= 1",
        ".monitoringUnknown == false",
        ".probeFailureCategory == null",
        ".rawProbeDataRetained == false",
    ] {
        assert!(
            playbook.contains(playbook_boundary),
            "macOS acceptance playbook is missing a review boundary: {playbook_boundary}"
        );
    }

    let phase_1 = playbook
        .split("### Phase 1: bind the candidate, run quiet, then stop for review")
        .nth(1)
        .unwrap()
        .split("### Phase 2: require quiet review, run deliberate, then stop for review")
        .next()
        .unwrap();
    let phase_1_readiness = phase_1.find("QUIET_READINESS_JSON=").unwrap();
    let phase_1_assertion = phase_1
        .find("assert_quiet_readiness_record \"$QUIET_READINESS_JSON\"")
        .unwrap();
    let phase_1_candidate = phase_1
        .find("\"$SERVER\" \"$HELPER\" \"$QUIET_DIR\"")
        .unwrap();
    assert!(phase_1_readiness < phase_1_assertion && phase_1_assertion < phase_1_candidate);

    let phase_2 = playbook
        .split("### Phase 2: require quiet review, run deliberate, then stop for review")
        .nth(1)
        .unwrap()
        .split("### Phase 3: require both reviews and finalize create-once")
        .next()
        .unwrap();
    let phase_2_review = phase_2
        .find(": \"${QUIET_REVIEWED_RESULT_SHA256:?complete Phase 1 visual review first}\"")
        .unwrap();
    let phase_2_readiness = phase_2.find("DELIBERATE_READINESS_JSON=").unwrap();
    let phase_2_assertion = phase_2
        .find("assert_quiet_readiness_record \"$DELIBERATE_READINESS_JSON\"")
        .unwrap();
    let phase_2_candidate = phase_2
        .find("\"$SERVER\" \"$HELPER\" \"$DELIBERATE_DIR\"")
        .unwrap();
    assert!(
        phase_2_review < phase_2_readiness
            && phase_2_readiness < phase_2_assertion
            && phase_2_assertion < phase_2_candidate
    );
    assert_eq!(
        playbook.matches("--quiet-readiness").count(),
        3,
        "the generic readiness example plus exactly two fresh pre-lane calls are required"
    );

    for integration in [&ci, &local] {
        assert!(
            integration.contains("node --check evidence/v0.12.66/computer/helper-evidence-rig.mjs"),
            "CI/local validation does not syntax-check the exact v0.12.66 macOS evidence rig"
        );
        assert!(
            integration
                .contains("node evidence/v0.12.66/computer/helper-evidence-rig.mjs --self-test")
        );
        assert!(
            !integration.contains("evidence/v0.12.20/computer/"),
            "active integration still targets the withdrawn v0.12.20 harness"
        );
    }
    for integration in [&ci, &local] {
        for source in ["HelperEvidenceFixture.swift", "SystemProbe.swift"] {
            assert!(
                integration.contains(&format!(
                    "xcrun swiftc -typecheck evidence/v0.12.66/computer/{source}"
                )),
                "macOS workflow does not typecheck {source}"
            );
        }
    }
    for integration in [&ci, &candidate, &local] {
        assert!(integration.contains("bash scripts/verify-macos-app-share-handoff-self-test.sh"));
    }
    for required in [
        "xcrun swiftc -typecheck \"$source_path\"",
        "xcrun swiftc \"$source_path\" -o \"$binary\"",
        "\"$binary\" --self-test >\"$stdout_path\" 2>\"$stderr_path\"",
        "(( status != 0 ))",
        "[[ -s \"$stderr_path\" ]]",
        "cmp -s \"$stdout_path\" \"$expected_path\"",
        "macOS app-share handoff self-test passed",
    ] {
        assert!(
            handoff_self_test.contains(required),
            "shared app-share self-test verifier is missing: {required}"
        );
    }
    assert!(ci.contains(
        "xcrun swiftc -typecheck evidence/v0.12.66/computer/PhysicalPointerHandoff.swift"
    ));
    for release_path in [&candidate, &local] {
        assert!(
            !release_path.contains("PhysicalPointerHandoff.swift"),
            "the physical-pointer adversarial helper must not become a release-path requirement"
        );
    }
    assert!(candidate.contains("bash scripts/verify-release-acceptance-evidence.sh --self-test"));
    assert!(candidate.contains("\"macOS native validation\""));

    for field in [
        "schemaVersion",
        "kind",
        "productVersion",
        "requestId",
        "createdAt",
        "expiresAt",
        "runnerPid",
        "promptPid",
        "expectedBundleIdentifier",
        "expectedWindowTitle",
        "expectedButtonText",
        "expectedButtonAccessibilityIdentifier",
        "expectedButtonEnabledAfterDelivery",
        "exactAppObserved",
        "exactWindowObserved",
        "requestDelivered",
        "panelOnScreen",
        "panelNonactivating",
        "notificationOnly",
        "exactAppShareRequired",
        "physicalHumanProvenanceRequired",
        "acceptedAsProductAuthority",
    ] {
        assert!(
            watcher.contains(&format!("\"{field}\"")),
            "macOS app-share watcher omits request field {field}"
        );
    }
    for field in [
        "acceptedAsAuthority",
        "buttonAccepted",
        "buttonActionObserved",
        "cryptographicToolIdentityClaimed",
        "physicalHumanProvenanceClaimed",
        "requestSha256",
        "buttonRemainedDisabledDuringProductAction",
        "handoffStateSequenceBound",
        "productActionCompletedAt",
        "productActionStartedAt",
        "startReceiptSha256",
    ] {
        assert!(
            watcher.contains(&format!("\"{field}\"")),
            "macOS app-share watcher omits chained receipt field {field}"
        );
    }

    for forbidden in [
        "writeFile(",
        "appendFile(",
        "rename(",
        "unlink(",
        "rm(",
        "mkdir(",
        "process.stdin",
        "readline",
        "child_process",
        "acceptedAsAuthority: true",
        "acceptedAsProductAuthority: true",
        "--ack",
    ] {
        assert!(
            !watcher.contains(forbidden),
            "read-only macOS app-share watcher contains forbidden primitive: {forbidden}"
        );
    }
}

#[test]
fn release_license_inventory_is_locked_sanitized_and_shipped() {
    let cargo = source("Cargo.toml");
    let lockfile = source("Cargo.lock");
    let about = source("about.toml");
    let notices = source("THIRD_PARTY_LICENSES.txt");
    let checker = source("scripts/check-licenses.sh");
    let package = source("scripts/package-extension.sh");
    let verifier = source("scripts/verify-release-assets.sh");
    let macos_verifier = source("scripts/verify-macos-artifacts.sh");

    assert!(!cargo.contains("dirs ="));
    assert!(lockfile.contains("name = \"tray-icon\""));
    assert!(about.contains("ignore-build-dependencies = true"));
    assert!(about.contains("ignore-dev-dependencies = true"));
    assert!(checker.contains("cargo about generate about.hbs --locked --fail"));
    assert!(checker.contains("LC_ALL=C awk"));
    assert!(checker.contains("if [[ \"$mode\" == --write ]]"));
    assert!(notices.contains("Local Browser Bridge third-party licenses"));
    for forbidden in [
        "option-ext",
        "Mozilla Public License",
        "/Users/",
        "\\Users\\",
    ] {
        assert!(
            !notices.contains(forbidden),
            "dependency notice contains forbidden text: {forbidden}"
        );
    }
    assert!(package.contains("source_path=\"LICENSE\""));
    for required in [
        "selected_payloads.get(\"LICENSE\") != source.read()",
        "extension archive project license differs from LICENSE",
        "cmp -s \"$mac_stage/$notice\" \"$notice\"",
        "THIRD_PARTY_LICENSES.txt",
    ] {
        assert!(
            verifier.contains(required),
            "release verifier is missing license check: {required}"
        );
    }
    assert!(verifier.contains("bash scripts/verify-macos-artifacts.sh"));
    assert!(macos_verifier.contains("--licenses"));
}

#[test]
fn release_asset_archive_readers_are_exact_bounded_and_fail_closed() {
    let verifier = source("scripts/verify-release-assets.sh");
    for required in [
        "maximum_entry_bytes = 16 * 1024 * 1024",
        "maximum_total_bytes = 64 * 1024 * 1024",
        "extension archive inventory is duplicated or noncanonical",
        "extension archive exceeds its bounded uncompressed size",
        "item.flag_bits & 0x1",
        "item.compress_type not in (zipfile.ZIP_STORED, zipfile.ZIP_DEFLATED)",
        "maximum_member_bytes = 128 * 1024 * 1024",
        "maximum_total_bytes = 256 * 1024 * 1024",
        "macOS archive contains global PAX metadata",
        "macOS archive path is duplicated, unexpected, or PAX-overridden",
        "macOS archive does not contain the exact canonical inventory",
        "os.O_EXCL | getattr(os, \"O_NOFOLLOW\", 0)",
        "source.read(1)",
        "os.fsync(output.fileno())",
        "Extension archive changed while it was inspected.",
        "macOS archive changed while it was inspected.",
    ] {
        assert!(
            verifier.contains(required),
            "release asset verifier is missing bounded archive invariant: {required}"
        );
    }
    for forbidden in [
        "unzip -Z1",
        "unzip -tq",
        "unzip -p",
        "zipinfo",
        "tar -tzf",
        "tar -tvzf",
        "tar -xzf",
        "extractall(",
        "getmembers()",
    ] {
        assert!(
            !verifier.contains(forbidden),
            "release asset verifier uses an unbounded archive primitive: {forbidden}"
        );
    }

    let extension_before = verifier.find("extension_archive_sha256_before=").unwrap();
    let extension_reader = verifier
        .find("python3 - \"$extension_archive\" \"$version\"")
        .unwrap();
    let extension_after = verifier
        .find("Extension archive changed while it was inspected.")
        .unwrap();
    assert!(extension_before < extension_reader && extension_reader < extension_after);
    let mac_before = verifier.find("macos_archive_sha256_before=").unwrap();
    let mac_reader = verifier
        .find("python3 - \"$macos_archive\" \"$mac_stage\"")
        .unwrap();
    let mac_after = verifier
        .find("macOS archive changed while it was inspected.")
        .unwrap();
    assert!(mac_before < mac_reader && mac_reader < mac_after);
}

#[test]
fn two_phase_release_is_tagless_reviewed_low_cost_and_provenance_bound() {
    let ci = source(".github/workflows/ci.yml");
    let candidate = source(".github/workflows/deploy.yml");
    let publication = source(".github/workflows/publish.yml");
    let publisher = source("scripts/publish-release.sh");
    let verifier = source("scripts/verify-release-assets.sh");

    let ci_trigger = ci.split("permissions:").next().unwrap();
    assert!(ci_trigger.contains("on:\n  pull_request:\n  workflow_dispatch:"));
    assert!(!ci_trigger.contains("push:"));
    assert_eq!(workflow_job_ids(&ci), vec!["rust", "windows", "macos"]);
    for name in [
        "name: Rust, extension, and packaging",
        "name: Windows native validation",
        "name: macOS native validation",
    ] {
        assert!(
            ci.contains(name),
            "the three-lane CI contract is missing {name}"
        );
    }
    assert!(!ci.contains("actions/upload-artifact@"));
    assert!(ci.contains("cargo test --locked --all-targets"));
    assert!(ci.contains("cargo test --locked --target x86_64-pc-windows-msvc --all-targets"));
    assert!(ci.contains("bash scripts/verify-macos-build-host.sh"));

    let candidate_trigger = candidate.split("permissions:").next().unwrap();
    assert!(candidate_trigger.contains("on:\n  workflow_dispatch:"));
    assert!(!candidate_trigger.contains("push:"));
    assert_eq!(
        workflow_job_ids(&candidate),
        vec!["verify", "windows", "macos", "assemble"]
    );
    let candidate_verify = job_section(&candidate, "verify");
    assert!(candidate_verify.contains("pull-requests: read"));
    assert!(!candidate.contains("environment:"));
    assert!(!candidate.contains("contents: write"));
    for forbidden in [
        "gh release create",
        "gh release upload",
        "gh release delete",
        "git tag ",
        "git push ",
        "publish-release.sh publish",
        "refs/tags/${{",
    ] {
        assert!(
            !candidate.contains(forbidden),
            "tagless candidate workflow contains publication primitive: {forbidden}"
        );
    }
    for source_gate in [
        "[[ \"$GITHUB_REF\" == \"refs/heads/main\" ]]",
        "test \"$GITHUB_SHA\" = \"$SOURCE_REF\"",
        "test \"$(git rev-parse origin/main)\" = \"$SOURCE_REF\"",
        "commits/$SOURCE_REF/pulls?per_page=100",
        "repos/$GITHUB_REPOSITORY/pulls/$reviewed_number",
        "repos/$GITHUB_REPOSITORY/git/commits/$reviewed_head",
        "test \"$(git rev-parse \"$SOURCE_REF^{tree}\")\" = \"$reviewed_tree\"",
        ".base.ref == \"main\"",
        "source must belong to exactly one merged main PR",
        "pull request detail does not bind the reviewed source",
        "commits/$reviewed_head/check-runs?per_page=100",
        "\"Rust, extension, and packaging\"",
        "\"Windows native validation\"",
        "\"macOS native validation\"",
        ".head_sha == $source",
        ".status == \"completed\"",
        ".conclusion == \"success\"",
        ".app.slug == \"github-actions\"",
    ] {
        assert!(
            candidate.contains(source_gate),
            "candidate source/CI binding is missing {source_gate}"
        );
    }
    let association_lookup = candidate
        .find("commits/$SOURCE_REF/pulls?per_page=100")
        .unwrap();
    let detail_lookup = candidate
        .find("repos/$GITHUB_REPOSITORY/pulls/$reviewed_number")
        .unwrap();
    let tree_lookup = candidate
        .find("repos/$GITHUB_REPOSITORY/git/commits/$reviewed_head")
        .unwrap();
    assert!(
        association_lookup < detail_lookup && detail_lookup < tree_lookup,
        "candidate verification must resolve the associated PR detail before comparing source trees"
    );
    assert!(
        !candidate.contains("merge_commit_sha"),
        "candidate verification must not depend on the field removed by GitHub API 2026-03-10"
    );
    assert!(candidate.contains("] | all(. as $required | any($response.check_runs[];"));
    assert!(
        !candidate.contains("all(.[] as $required"),
        "jq all(condition) already evaluates each array element and must not iterate each check name as a second array"
    );
    for asset in [
        "local-browser-bridge-v${version}-windows-x86_64.exe",
        "local-computer-helper-v${version}-windows-x86_64.exe",
        "local-browser-bridge-v${version}-macos-universal.tar.gz",
        "local-browser-bridge-extension-v${VERIFIED_VERSION}.zip",
        "SHA256SUMS.txt",
    ] {
        assert!(
            candidate.contains(asset),
            "candidate omits exact asset {asset}"
        );
    }
    assert_eq!(
        candidate
            .matches("actions/attest-build-provenance@")
            .count(),
        4
    );
    assert_eq!(
        candidate
            .lines()
            .filter(|line| line.trim() == "retention-days: 1")
            .count(),
        3
    );
    assert_eq!(
        candidate
            .lines()
            .filter(|line| line.trim() == "retention-days: 14")
            .count(),
        1
    );
    assert!(candidate.contains("name: release-candidate"));
    assert!(candidate.contains("for subject in dist/*; do verify_attestation \"$subject\"; done"));
    assert!(candidate.contains("--source-ref refs/heads/main"));
    assert!(candidate.contains("--source-digest \"$VERIFIED_SOURCE_SHA\""));
    assert!(
        candidate.contains("--signer-workflow \"$GITHUB_REPOSITORY/.github/workflows/deploy.yml\"")
    );
    assert!(candidate.contains("--deny-self-hosted-runners"));

    assert_eq!(workflow_job_ids(&publication), vec!["preflight", "release"]);
    let preflight = job_section(&publication, "preflight");
    let release = job_section(&publication, "release");
    assert!(!preflight.contains("environment:"));
    assert!(!preflight.contains("contents: write"));
    assert!(preflight.contains("bash scripts/fetch-verify-release-candidate.sh"));
    assert!(preflight.contains("bash scripts/verify-release-acceptance-evidence.sh"));
    assert!(preflight.contains("jq -e '.schemaVersion == 3'"));
    assert!(preflight.contains("bash scripts/publish-release.sh prepare"));
    assert!(preflight.contains("bash scripts/publish-release.sh check-remote"));
    assert!(preflight.contains("retention-days: 1"));
    assert!(preflight.contains("compression-level: 0"));
    assert!(!preflight.contains("publish-release.sh publish"));
    assert!(release.contains("environment:\n      name: release"));
    assert!(release.contains("contents: write"));
    assert!(release.contains("needs: preflight"));
    assert!(release.contains("artifact-ids: ${{ needs.preflight.outputs.approval_artifact_id }}"));
    assert!(release.contains("bash scripts/publish-release.sh publish"));
    assert_eq!(
        publication
            .matches("environment:\n      name: release")
            .count(),
        1
    );
    assert_eq!(publication.matches("contents: write").count(), 1);
    assert_eq!(publication.matches("publish-release.sh publish").count(), 1);

    for publication_contract in [
        "readonly CANDIDATE_REF=\"refs/heads/main\"",
        ".schemaVersion == 3",
        ".workflowEvent == \"workflow_dispatch\"",
        ".workflowRef == \"refs/heads/main\"",
        ".workflowPath == \".github/workflows/deploy.yml\"",
        ".workflowRunAttempt == $run_attempt",
        "create_or_verify_tag",
        "assert_tag",
        "draft:true",
        ".draft == false and .immutable == true",
        "gh release upload \"$RELEASE_TAG\" \"$approved/$name\"",
        "gh release download \"$RELEASE_TAG\"",
        "cmp -s \"$approved/$name\" \"$scratch/downloads/$name\"",
        "gh release verify-asset \"$RELEASE_TAG\"",
        "gh release verify \"$RELEASE_TAG\"",
        ".verificationResult.statement.predicate.repository == $repository",
        ".verificationResult.statement.predicate.tag == $tag",
        ".digest.sha1 == $tag_object_sha",
        "--source-ref \"$CANDIDATE_REF\"",
        "--source-digest \"$VERIFIED_SOURCE_SHA\"",
        "--signer-workflow \"$REPOSITORY/$CANDIDATE_WORKFLOW\"",
        "--deny-self-hosted-runners",
        "Canonical acceptance receipt SHA-256",
        "assert_repository_release_policy()",
        "repos/$REPOSITORY/immutable-releases",
        "test \"$enabled\" = true",
        ".conditions.ref_name.include == [\"refs/tags/v*\"]",
        ".conditions.ref_name.exclude == []",
        "([.rules[].type] | index(\"update\") != null and index(\"deletion\") != null)",
        "(.bypass_actors | type == \"array\" and length == 0)",
        ".current_user_can_bypass == \"never\"",
        "release tags are not protected by one unbypassable update/deletion ruleset",
    ] {
        assert!(
            publisher.contains(publication_contract),
            "protected publication helper is missing {publication_contract}"
        );
    }
    assert!(!publisher.contains("gh release delete"));
    assert!(!publisher.contains("--method DELETE"));
    assert_eq!(
        publisher
            .matches("assert_repository_release_policy")
            .count(),
        5,
        "the policy function plus preflight, protected-job entry, pre-tag, and pre-publication calls must remain present"
    );

    let protected_publish = publisher.split("publish_approved() {").nth(1).unwrap();
    assert!(
        protected_publish
            .find("assert_repository_release_policy")
            .unwrap()
            < protected_publish.find("create_or_verify_tag").unwrap(),
        "the protected job must prove immutable Releases and unbypassable tag protection before creating a tag"
    );
    let draft_publication = publisher
        .split("create_or_recover_release() {")
        .nth(1)
        .unwrap()
        .split("\n}\n\nverify_published_release()")
        .next()
        .unwrap();
    assert!(
        draft_publication
            .rfind("assert_repository_release_policy")
            .unwrap()
            < draft_publication.find("gh api --method PATCH").unwrap(),
        "repository release policy must be rechecked immediately before publishing a draft"
    );

    assert!(
        verifier
            .contains("Release checksum manifest is missing, empty, linked, or not a regular file")
    );
    assert!(verifier.contains("Release asset is missing, empty, linked, or not a regular file"));
    assert!(verifier.contains("Release directory contains an unexpected file set."));
    assert!(verifier.contains("Checksum manifest must contain exactly four canonical lines."));
    assert!(verifier.contains("Checksum manifest bytes are not canonical LF-terminated ASCII."));
}

#[test]
fn release_requires_canonical_schema_three_receipt_and_committed_evidence() {
    let publication = source(".github/workflows/publish.yml");
    let verifier = source("scripts/verify-release-acceptance-evidence.sh");
    let publisher = source("scripts/publish-release.sh");

    for required in [
        "acceptance_receipt:\n        description: Canonical one-line schema-3 acceptance receipt JSON",
        "ACCEPTANCE_RECEIPT: ${{ inputs.acceptance_receipt }}",
        "CANDIDATE_RUN_ID: ${{ inputs.candidate_run_id }}",
        "CANDIDATE_RUN_ATTEMPT: ${{ inputs.candidate_run_attempt }}",
        "RELEASE_TAG: v${{ inputs.version }}",
        "VERIFIED_SOURCE_SHA: ${{ inputs.source_sha }}",
        "test \"$(jq -c . \"$receipt\")\" = \"$(<\"$receipt\")\"",
        "jq -e '.schemaVersion == 3' \"$receipt\"",
        "bash scripts/verify-release-acceptance-evidence.sh",
        "bash scripts/publish-release.sh prepare",
        "acceptance_receipt_sha256: ${{ steps.approval.outputs.acceptance_receipt_sha256 }}",
        "EXPECTED_RECEIPT_SHA256: ${{ needs.preflight.outputs.acceptance_receipt_sha256 }}",
    ] {
        assert!(
            publication.contains(required),
            "schema-3 publication gate is missing {required}"
        );
    }
    for forbidden in [
        "LBB_RELEASE_ACCEPTANCE_V1",
        "LBB_RELEASE_ACCEPTANCE_V2",
        "vars.LBB_RELEASE_ACCEPTANCE",
        "secrets.LBB_RELEASE_ACCEPTANCE",
        "VERIFIED_TAG_SHA",
        "tagObjectSha",
    ] {
        assert!(
            !publication.contains(forbidden),
            "publication still depends on obsolete or mutable receipt state: {forbidden}"
        );
    }

    let canonical_keys = [
        "schemaVersion",
        "version",
        "releaseTag",
        "sourceSha",
        "workflowRunId",
        "workflowRunAttempt",
        "releaseCandidateArtifactId",
        "releaseCandidateArtifactZipSha256",
        "checksumManifestSha256",
        "evidenceRef",
        "evidenceCommitSha",
        "macosPassed",
        "macosAcceptanceSha256",
        "macosQuietResultSha256",
        "macosDeliberateConcurrencyResultSha256",
        "windowsPassed",
        "windowsResultSha256",
        "stockChromePassed",
        "stockChrome",
        "stockChromeResultSha256",
    ];
    for key in canonical_keys {
        assert!(
            verifier.contains(&format!("\"{key}\"")),
            "canonical schema-3 receipt omits {key}"
        );
    }
    for required in [
        "keys_unsorted == [",
        ".schemaVersion == 3",
        ".releaseTag == (\"v\" + .version)",
        ".releaseTag == $release_tag",
        ".sourceSha == $source_sha",
        ".workflowRunId == $run_id",
        ".workflowRunAttempt == $run_attempt",
        ".checksumManifestSha256 == $manifest_sha256",
        ".macosPassed == true",
        ".windowsPassed == true",
        ".stockChromePassed == true",
        ".stockChrome == true",
        "refs/heads/evidence/",
        "macos/macos-acceptance.json",
        "macos/quiet/helper-results.json",
        "macos/deliberate-concurrency/helper-results.json",
        "windows/computer/summary.json",
        "windows/browser/browser-acceptance.json",
        "workflowEvent: \"workflow_dispatch\"",
        "workflowRef: \"refs/heads/main\"",
        "workflowPath: \".github/workflows/deploy.yml\"",
    ] {
        assert!(
            verifier.contains(required),
            "schema-3 verifier is missing {required}"
        );
    }
    assert!(!verifier.contains("tagObjectSha"));
    assert!(!verifier.contains("VERIFIED_TAG_SHA"));

    for receipt_binding in [
        "acceptanceReceiptSha256",
        "test \"$(sha256_file \"$directory/acceptance-receipt.json\")\" = \"$expected_receipt_sha\"",
        "Canonical acceptance receipt SHA-256: \\`$receipt_sha256\\`",
        "Candidate workflow: run \\`$CANDIDATE_RUN_ID\\`, attempt \\`$CANDIDATE_RUN_ATTEMPT\\`",
        "Accepted source: [\\`$VERIFIED_SOURCE_SHA\\`]",
        "assert_release_identity",
    ] {
        assert!(
            publisher.contains(receipt_binding),
            "immutable publication does not preserve receipt/source binding: {receipt_binding}"
        );
    }
}

#[test]
fn release_evidence_verifier_fails_closed_on_artifact_or_commit_substitution() {
    let verifier = source("scripts/verify-release-acceptance-evidence.sh");
    for required in [
        "actions/runs/$CANDIDATE_RUN_ID/attempts/$CANDIDATE_RUN_ATTEMPT",
        "actions/runs/$CANDIDATE_RUN_ID/attempts/$CANDIDATE_RUN_ATTEMPT/jobs?per_page=100",
        "actions/runs/$CANDIDATE_RUN_ID/artifacts?per_page=100",
        "actions/artifacts/$artifact_id/zip",
        "raw release-candidate artifact ZIP SHA-256 mismatch",
        "cmp -s \"$extracted/$asset\" \"$candidate_dir/$asset\"",
        "git ls-remote --refs origin \"$evidence_ref\"",
        "git -c protocol.version=2 fetch --quiet --no-tags origin \"$evidence_ref\"",
        "evidence commit must have the verified source as its sole parent",
        "git diff-tree --no-commit-id --name-status -r -z",
        "test \"$status\" = A",
        "test \"$mode\" = 100644 && test \"$type\" = blob",
        "evidence tree contains a symlink, executable, submodule, or non-blob",
        "evidence commit contains an unreferenced or unexpected sidecar",
        "scan_evidence_for_leaks",
        "retained evidence contains a forbidden",
        "raw release-candidate artifact ZIP byte count differs from GitHub metadata",
        "receipt artifact ZIP SHA-256 differs from GitHub metadata",
        "--format json > \"$attestation_json\"",
        "runInvocationURI == $invocation",
        "verify_png_dimensions()",
        "verify_png_dimensions \"$lane_root/$filename\" \"$width\" \"$height\"",
        "verify_png_dimensions \"$image\" \"$image_width\" \"$image_height\"",
        "maximum_entry_bytes = 256 * 1024 * 1024",
        "maximum_total_bytes = 512 * 1024 * 1024",
        "release-candidate artifact ZIP exceeds its bounded uncompressed size",
        "os.O_EXCL | getattr(os, \"O_NOFOLLOW\", 0)",
        "maximum_member_bytes = 128 * 1024 * 1024",
        "maximum_total_bytes = 256 * 1024 * 1024",
        "macOS package contains global PAX metadata",
        "macOS package does not have the exact independently inspected inventory",
        "macOS deliberate-concurrency lane did not start after the quiet lane passed",
        "aggregate lane start timestamp differs from its raw result",
        "validate_macos_authority_assertion_contract()",
        "app-share receipt retained the exact persistent share",
        "post-handoff share action authority is fresh and exact",
        "app-share handoff and frame refresh caused no target mutation",
        "post-handoff share action authority remained fresh at dispatch",
        "self-test accepted deliberate app-share authority that was not refreshed after receipt",
        "self-test accepted deliberate app-share authority that was stale at dispatch",
        "self-test accepted a missing deliberate authority assertion",
        "self-test accepted a duplicate authority assertion name",
        ".schemaVersion == 9",
        ".aggregateChecks.passingResultSchemaVersion == 9",
        ".aggregateChecks.inventoryFileCount == 19",
        "macos-app-share-concurrency-handoff-request.json",
        "macos-app-share-concurrency-handoff-start.json",
        "macos-app-share-concurrency-handoff-complete.json",
    ] {
        assert!(
            verifier.contains(required),
            "release evidence substitution defense is missing {required}"
        );
    }
    for lane_assertion in [
        ".pointerEvidence.requestedLane == $lane",
        ".pointerEvidence.quietObserved == true",
        ".pointerEvidence.unknownObserved == false",
        ".pointerEvidence.concurrentSharedSeatActivityObserved == false",
        ".quietSeatStabilization.required == true",
        ".quietSeatStabilization.completed == true",
        ".quietSeatStabilization.completedBeforeCandidateExecution == true",
        ".quietSeatStabilization.stableDurationMilliseconds >= 30000",
        ".quietSeatStabilization.observedSamples >= 61",
        ".quietSeatStabilization.stableTransitions >= 60",
        ".quietSeatStabilization.monitoringUnknown == false",
        "(if $lane == \"quiet\" then",
        ".appShareHandoff == {",
        "requested: false,",
        "requestPublicationAcknowledged: false,",
        "startReceiptAcknowledged: false,",
        "completePublicationAcknowledged: false,",
        "requested: true,",
        "requestPublicationAcknowledged: true,",
        "startReceiptAcknowledged: true,",
        "completePublicationAcknowledged: true,",
        "promptClosed: true,",
        "exactAppBundleObserved: true,",
        "exactWindowObserved: true,",
        "exactButtonObserved: true,",
        "buttonDisabledAfterAction: true,",
        "acceptanceButtonActionObserved: true,",
        "appShareSurfaceObservedAtProductBoundaries: true,",
        "sharedHidInputObserved: null,",
        "sharedHidInputObserved: false,",
        "sampledSharedContextUnchanged: true,",
        "authorityRefreshedAfterReceipt: false,",
        "authorityFreshAtDispatch: false,",
        "authorityRefreshedAfterReceipt: true,",
        "authorityFreshAtDispatch: true,",
        "actionDispatched: true,",
        "targetPostconditionObserved: true,",
        "productBoundaryQuiet: true,",
        "independentBoundaryQuiet: true,",
        "physicalHumanProvenanceClaimed: false,",
        "cryptographicToolIdentityClaimed: false,",
        "orchestrationNotProductControl: true,",
        "markerNotificationOnly: false,",
        "markerAcceptedAsProductAuthority: false,",
        "rawAppIdentityRetainedInResult: false,",
        "rawPointerDataRetained: false",
    ] {
        assert!(verifier.contains(lane_assertion));
    }
    assert!(verifier.contains("Release acceptance evidence verifier self-test passed."));
}

#[test]
fn release_evidence_verifier_executes_adversarial_replay_and_decoder_tests() {
    #[cfg(not(target_os = "windows"))]
    {
        let output = Command::new("bash")
            .args([
                "scripts/verify-release-acceptance-evidence.sh",
                "--self-test",
            ])
            .output()
            .expect("release evidence verifier self-test must start");
        assert!(
            output.status.success(),
            "release evidence verifier self-test failed:\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains("Release acceptance evidence verifier self-test passed.")
        );
    }

    let verifier = source("scripts/verify-release-acceptance-evidence.sh");
    for required in [
        "self-test accepted release-candidate evidence replayed from another workflow attempt",
        "self-test accepted a truncated Windows step inventory",
        "self-test accepted a reordered Windows step inventory",
        "self-test accepted an arm receipt without the request ID",
        "self-test accepted an empty stock-Chrome method matrix",
        "self-test accepted a macOS screenshot hash replayed across lanes",
        "self-test accepted decoded macOS pixels replayed across lanes",
        "encoding-replay.png",
        "self-test failed to construct a byte-distinct PNG encoding replay",
        "self-test accepted an undecodable zero-length PNG IDAT",
    ] {
        assert!(
            verifier.contains(required),
            "executable adversarial verifier self-test is missing: {required}"
        );
    }
}

#[test]
fn release_evidence_gate_requires_exact_current_attempt_and_complete_ui_proofs() {
    let verifier = source("scripts/verify-release-acceptance-evidence.sh");
    for required in [
        "assert_release_candidate_binding \"$summary\" '.releaseCandidateBinding'",
        "assert_release_candidate_binding \"$final\" '.releaseCandidateBinding'",
        "assert_release_candidate_binding \"$preflight\" '.releaseCandidateBinding'",
        "assert_release_candidate_binding \"$postflight\" '.releaseCandidateBinding'",
        "assert_release_candidate_binding \"$helper\" '.releaseCandidateBinding'",
        "assert_release_candidate_binding \"$approval\" '.releaseCandidateBinding'",
        "assert_release_candidate_binding \"$review\" '.releaseCandidateBinding'",
        "assert_release_candidate_binding \"$operator\" '.releaseCandidateBinding'",
        "assert_release_candidate_binding \"$sidecar\" '.releaseCandidateBinding'",
        "workflowRunAttempt: $workflow_run_attempt",
        "length == 62",
        "62-foreground-cursor-focus-desktop-invariants.json",
        "test \"${#windows_screenshots[@]}\" = 20",
        "stableSamplesRequired == 3",
        "methodCount == 25",
        "page.handleDialog",
        "operatorRecordSha256",
        "browser-04-stop-paused.png",
        "browser-05-cancel-paused.png",
        "browser-06-post-handback-resume.png",
        ".retainedEvidence.inputFileCount == 21 and .retainedEvidence.finalFileCount == 22",
        "external-surface-preflight.json",
        "external-surface-postflight.json",
        "scoped-action-approval.json",
        "independent-visual-review.json",
        ".response.deliveredBy == \"user-via-orchestrator\"",
        ".operatorExchange.requestCount == (.operatorExchange.statusDecisionCount + .operatorExchange.freshFrameDecisionCount + 1)",
        ".response.orchestratorSessionRef != .operatorExchange.executorSessionRef",
        ".response.orchestratorSessionRef != .operatorExchange.reviewerSessionRef",
        ".independentVisualReview.noUncertaintyReported == true",
        ".independentVisualReview.visualJudgmentNotPixelSafetyProof == true",
        "source-schema complete contract replay",
        "zlib.decompressobj()",
        "PNG IDAT does not decode to the claimed raster",
        "PNG decoded pixels do not match the aggregate hash",
        "pixel-bound PNG contains unexpected metadata or ancillary chunks",
        "twelve globally file- and decoded-pixel-distinct screenshots",
        ".pixelSha256",
        ".aggregateChecks.screenshotPixelHashesMatched == true",
        ".automatedTextInspectionPerformed == false",
        ".independentVisualReviewRequired == true",
        ".independentVisualReviewCompleted == true",
        "stock-Chrome screenshot sidecar is not the exact independent digest-bound review schema",
        "extract_macos_candidate_facts",
        "LC_CODE_SIGNATURE",
        ".package.serverSha256 == $package_facts[0].serverSha256",
        ".package.helperSha256 == $package_facts[0].helperSha256",
    ] {
        assert!(
            verifier.contains(required),
            "release evidence gate is missing the exact proof: {required}"
        );
    }
}

#[test]
fn release_reruns_never_delete_and_recover_only_byte_exact_drafts() {
    let publisher = source("scripts/publish-release.sh");
    for binding in [
        "assert_release_identity \"$release_json\" \"$notes\"",
        "assert_release_assets \"$release_json\" \"$approved\" true",
        "assert_release_assets \"$release_json\" \"$approved\" false",
        "validate_approval \"$approved\" \"$expected_receipt_sha\"",
        "verify_release_assets_in_bundle \"$approved\"",
        "verify_attestation \"$approved/$name\" \"$attestation\"",
        "a release exists without the exact annotated tag",
        "release is not an exact recoverable draft or immutable publication",
    ] {
        assert!(
            publisher.contains(binding),
            "recoverable publication ownership is missing {binding}"
        );
    }
    assert_eq!(
        publisher
            .matches("gh release upload \"$RELEASE_TAG\"")
            .count(),
        1
    );
    assert!(!publisher.contains("gh release delete"));
    assert!(!publisher.contains("--method DELETE"));
    assert!(!publisher.contains("--cleanup-tag"));

    let recovery = publisher
        .split("create_or_recover_release() {")
        .nth(1)
        .unwrap()
        .split("\n}\n\nverify_published_release()")
        .next()
        .unwrap();
    let identity = recovery
        .find("assert_release_identity \"$release_json\" \"$notes\"")
        .unwrap();
    let subset = recovery
        .find("assert_release_assets \"$release_json\" \"$approved\" true")
        .unwrap();
    let draft_gate = recovery
        .find(".draft == true and .immutable == false")
        .unwrap();
    let upload = recovery
        .find("gh release upload \"$RELEASE_TAG\" \"$approved/$name\"")
        .unwrap();
    let exact = recovery[upload..]
        .find("assert_release_assets \"$release_json\" \"$approved\" false")
        .map(|offset| upload + offset)
        .unwrap();
    let publish = recovery
        .find("'{\"draft\":false,\"make_latest\":\"true\"}'")
        .unwrap();
    assert!(identity < subset);
    assert!(subset < draft_gate);
    assert!(draft_gate < upload);
    assert!(upload < exact);
    assert!(exact < publish);
}

#[test]
fn release_reruns_resume_only_an_exact_immutable_publication() {
    let publisher = source("scripts/publish-release.sh");
    let recovery = publisher
        .split("create_or_recover_release() {")
        .nth(1)
        .unwrap()
        .split("\n}\n\nverify_published_release()")
        .next()
        .unwrap();

    let published_gate = recovery.find("if jq -e '.draft == false'").unwrap();
    let draft_gate = recovery
        .find(".draft == true and .immutable == false")
        .unwrap();
    let published = &recovery[published_gate..draft_gate];
    assert!(published.contains(".immutable == true"));
    assert!(published.contains("assert_release_assets \"$release_json\" \"$approved\" false"));
    assert!(published.contains("return 0"));
    assert!(!published.contains("gh release upload"));

    let verification = publisher
        .split("verify_published_release() {")
        .nth(1)
        .unwrap()
        .split("\n}\n\ncheck_remote_state()")
        .next()
        .unwrap();
    for required in [
        ".draft == false and .prerelease == false and .immutable == true",
        "assert_release_identity \"$release_json\" \"$notes\"",
        "assert_release_assets \"$release_json\" \"$approved\" false",
        "assert_tag",
        "gh release download \"$RELEASE_TAG\"",
        "cmp -s \"$approved/$name\" \"$scratch/downloads/$name\"",
        "gh release verify-asset \"$RELEASE_TAG\"",
        "verify_attestation \"$scratch/downloads/$name\" \"$attestation\"",
        "bash scripts/verify-release-assets.sh \"$RELEASE_VERSION\" \"$scratch/downloads\" --static-only",
        "gh release verify \"$RELEASE_TAG\"",
        ".digest.sha1 == $tag_object_sha",
    ] {
        assert!(
            verification.contains(required),
            "immutable rerun verification is missing {required}"
        );
    }
}

#[test]
fn macos_artifacts_enforce_the_supported_floor_per_macho_slice() {
    let verifier = source("scripts/verify-macos-artifacts.sh");
    let release = source(".github/workflows/deploy.yml");
    let local = source("scripts/deploy.sh");
    let archive = source("scripts/verify-release-assets.sh");

    for required in [
        "lipo -archs",
        "otool -l -arch",
        "LC_BUILD_VERSION",
        "LC_VERSION_MIN_MACOSX",
        "deployment_target_for_slice",
        "deployment_target\" != \"13.0",
        "for architecture in arm64 x86_64",
        "codesign --verify --strict",
    ] {
        assert!(
            verifier.contains(required),
            "macOS verifier is missing {required}"
        );
    }
    for caller in [release, local, archive] {
        assert!(caller.contains("bash scripts/verify-macos-artifacts.sh"));
    }
}

#[test]
fn macos_candidate_binding_is_static_only_while_default_verification_executes_runtime_checks() {
    let archive = source("scripts/verify-release-assets.sh");
    let macos = source("scripts/verify-macos-artifacts.sh");
    let workflow = source(".github/workflows/deploy.yml");
    let local = source("scripts/deploy.sh");

    for required in [
        "verification_mode=\"runtime\"",
        "macos_verifier_arguments=(\"$version\" \"$mac_server\" \"$mac_helper\" \"$mac_desktop\")",
        "if [[ \"$verification_mode\" == \"static-only\" ]]; then",
        "macos_verifier_arguments+=(\"--static-only\")",
        "bash scripts/verify-macos-artifacts.sh \"${macos_verifier_arguments[@]}\"",
    ] {
        assert!(
            archive.contains(required),
            "release-asset verifier is missing static-only propagation contract: {required}"
        );
    }
    for required in [
        "verification_mode=\"runtime\"",
        "elif (( $# == 5 )) && [[ \"$5\" == \"--static-only\" ]]; then",
        "if [[ \"$verification_mode\" == \"runtime\" ]]; then",
        "\"$(\"$server_path\" --version)\"",
        "\"$(\"$helper_path\" --version)\"",
        "license_report=\"$(\"$executable\" --licenses)\"",
        "without candidate execution",
    ] {
        assert!(
            macos.contains(required),
            "macOS verifier is missing runtime/static separation contract: {required}"
        );
    }

    let static_structure = macos
        .find("for executable in \"$server_path\" \"$helper_path\" \"$desktop_path\"; do")
        .unwrap();
    let api_audit = macos
        .find("  audit_helper_api_slice \"$architecture\"")
        .unwrap();
    let runtime_gate = macos
        .find("if [[ \"$verification_mode\" == \"runtime\" ]]; then")
        .unwrap();
    let first_runtime_execution = macos
        .find("if [[ \"$(\"$server_path\" --version)\"")
        .unwrap();
    let static_success = macos.find("without candidate execution").unwrap();
    assert!(
        static_structure < api_audit
            && api_audit < runtime_gate
            && runtime_gate < first_runtime_execution
            && first_runtime_execution < static_success,
        "all static Mach-O/signature/API checks must precede the guarded runtime-only block"
    );

    assert!(
        !workflow.contains("--static-only") && !local.contains("--static-only"),
        "build, package, and publication verification must retain default runtime checks"
    );
}

#[test]
fn packaged_macos_helper_freezes_targeted_input_apis_per_architecture() {
    let verifier = source("scripts/verify-macos-artifacts.sh");
    let release = source(".github/workflows/deploy.yml");
    let local = source("scripts/deploy.sh");
    let archive = source("scripts/verify-release-assets.sh");

    for required in [
        "for command in lipo otool codesign nm strings",
        "forbidden_helper_apis=(",
        "allowed_dynamic_lookup_symbols=(",
        "audit_helper_api_slice() {",
        "lipo -thin \"$architecture\" \"$helper_path\" -output \"$slice_path\"",
        "nm -u \"$slice_path\" > \"$undefined_report\"",
        "strings -a \"$slice_path\" > \"$strings_report\"",
        "report_mentions_api \"$api\" \"$undefined_report\" \"$strings_report\"",
        "grep -Fxq \"$api\" \"$strings_report\"",
        "grep -Eq '(^|[[:space:]])_dlsym$' \"$undefined_report\"",
        "observed_dynamic_symbols",
        "expected_dynamic_symbols",
        "for architecture in arm64 x86_64; do\n  audit_helper_api_slice \"$architecture\"",
    ] {
        assert!(
            verifier.contains(required),
            "macOS packaged-helper API verifier is missing {required}"
        );
    }

    let forbidden_block = verifier
        .split("forbidden_helper_apis=(")
        .nth(1)
        .unwrap()
        .split("\n)\n")
        .next()
        .unwrap();
    let forbidden_apis = [
        "CGWarpMouseCursorPosition",
        "CGDisplayMoveCursorToPoint",
        "CGAssociateMouseAndMouseCursorPosition",
        "CGDisplayHideCursor",
        "CGDisplayShowCursor",
        "CGEventPost",
        "CGEventTapPostEvent",
        "CGPostMouseEvent",
        "CGPostScrollWheelEvent",
        "CGPostKeyboardEvent",
        "IOHIDPostEvent",
        "IOHIDSetCursorEnable",
        "IOHIDSetCursorPosition",
        "CGEventPostToPSN",
        "CGEventPostToPid",
    ];
    assert_eq!(
        forbidden_block
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count(),
        forbidden_apis.len()
    );
    for forbidden in forbidden_apis {
        assert!(
            forbidden_block.lines().any(|line| line.trim() == forbidden),
            "macOS packaged-helper API verifier does not forbid {forbidden}"
        );
    }

    let allowed_block = verifier
        .split("allowed_dynamic_lookup_symbols=(")
        .nth(1)
        .unwrap()
        .split("\n)\n")
        .next()
        .unwrap();
    let allowed_lookups = [
        "CGEventSetWindowLocation",
        "CGSGetActiveSpace",
        "CGSMainConnectionID",
        "GetProcessPID",
        "SLEventPostToPid",
        "SLEventSetIntegerValueField",
        "SLPSPostEventRecordTo",
        "SLSGetActiveSpace",
        "SLSGetConnectionPSN",
        "SLSGetWindowOwner",
        "_SLPSGetFrontProcess",
    ];
    assert_eq!(
        allowed_block
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count(),
        allowed_lookups.len()
    );
    for allowed_lookup in allowed_lookups {
        assert!(
            allowed_block
                .lines()
                .any(|line| line.trim() == allowed_lookup),
            "macOS packaged-helper dynamic lookup allowlist omits {allowed_lookup}"
        );
    }

    let audit_call = verifier
        .find("  audit_helper_api_slice \"$architecture\"")
        .unwrap();
    let first_candidate_execution = verifier
        .find("if [[ \"$(\"$server_path\" --version)\"")
        .unwrap();
    assert!(
        audit_call < first_candidate_execution,
        "packaged-helper API inspection must complete before candidate execution"
    );

    let release_macos_job = release
        .split("- name: Build and verify universal server and helper app")
        .nth(1)
        .unwrap();
    assert!(
        release_macos_job
            .find("bash scripts/verify-macos-artifacts.sh")
            .unwrap()
            < release_macos_job
                .find("COPYFILE_DISABLE=1 tar --format ustar --no-xattrs -czf")
                .unwrap()
    );
    assert!(
        local
            .find("bash scripts/verify-macos-artifacts.sh")
            .unwrap()
            < local
                .find("COPYFILE_DISABLE=1 tar --format ustar --no-xattrs -czf")
                .unwrap()
    );
    assert_eq!(
        release_macos_job
            .matches("COPYFILE_DISABLE=1 tar --format ustar --no-xattrs -czf")
            .count(),
        1
    );
    assert_eq!(
        local
            .matches("COPYFILE_DISABLE=1 tar --format ustar --no-xattrs -czf")
            .count(),
        1
    );
    assert!(archive.contains("bash scripts/verify-macos-artifacts.sh"));
}

#[test]
fn packaged_windows_helper_rejects_global_input_and_requires_target_route_apis() {
    let verifier = source("scripts/verify-windows-artifacts.ps1");

    for required in [
        "function Assert-HelperInputSurface",
        "& $DumpBinPath /nologo /imports $Path",
        "[System.IO.File]::ReadAllBytes($Path)",
        "[System.Text.Encoding]::ASCII.GetString($bytes)",
        "[System.Text.Encoding]::Unicode.GetString($bytes)",
        "Assert-HelperInputSurface -Path $resolvedHelperPath -DumpBinPath $dumpbin",
    ] {
        assert!(
            verifier.contains(required),
            "Windows packaged-helper API verifier is missing {required}"
        );
    }

    for forbidden in [
        "'SendInput'",
        "'SetCursorPos'",
        "'SetPhysicalCursorPos'",
        "'mouse_event'",
        "'keybd_event'",
        "'SetForegroundWindow'",
        "'AttachThreadInput'",
        "'AllowSetForegroundWindow'",
        "'LockSetForegroundWindow'",
        "'SwitchToThisWindow'",
    ] {
        assert!(
            verifier.contains(forbidden),
            "Windows packaged-helper API verifier does not forbid {forbidden}"
        );
    }
    for required_api in [
        "'PostMessageW'",
        "'GetCursorPos'",
        "'OpenInputDesktop'",
        "'GetUserObjectInformationW'",
        "'RegisterRawInputDevices'",
        "'SetWindowsHookExW'",
    ] {
        assert!(
            verifier.contains(required_api),
            "Windows packaged-helper API verifier does not require {required_api}"
        );
    }

    let api_audit = verifier
        .find("Assert-HelperInputSurface -Path $resolvedHelperPath")
        .unwrap();
    let first_candidate_execution = verifier
        .find("$reportedVersion = (& $resolved --version")
        .unwrap();
    assert!(
        api_audit < first_candidate_execution,
        "Windows helper API inspection must complete before candidate execution"
    );
}

#[test]
fn workflows_disable_persisted_credentials_and_automatic_package_caches() {
    for path in [
        ".github/workflows/ci.yml",
        ".github/workflows/deploy.yml",
        ".github/workflows/publish.yml",
    ] {
        let workflow = source(path);
        assert_eq!(
            workflow.matches("actions/checkout@").count(),
            workflow.matches("persist-credentials: false").count(),
            "every checkout in {path} must drop its injected Git credentials"
        );
        assert_eq!(
            workflow.matches("actions/setup-node@").count(),
            workflow.matches("package-manager-cache: false").count(),
            "every Node setup in {path} must disable automatic package caching"
        );
    }

    for path in [
        ".github/workflows/deploy.yml",
        ".github/workflows/publish.yml",
    ] {
        let release = source(path);
        for forbidden in [
            "--source-digest \"${{",
            "= \"${{ needs.verify.outputs.tag_sha }}\"",
            "= \"${{ needs.verify.outputs.version }}\"",
            "VERIFIED_TAG_SHA",
        ] {
            assert!(
                !release.contains(forbidden),
                "release shell code in {path} contains obsolete or direct template expansion: {forbidden}"
            );
        }
    }
}

#[test]
fn mac_helper_bundle_has_a_stable_visible_identity() {
    let plist = source("packaging/macos/Info.plist.in");
    for required in [
        "dev.flrngel.local-browser-bridge.computer-helper",
        "<string>local-computer-helper</string>",
        "<string>Local Computer Helper</string>",
        "<string>@VERSION@</string>",
        "<key>LSMinimumSystemVersion</key>\n  <string>13.0</string>",
        "<key>NSScreenCaptureUsageDescription</key>",
    ] {
        assert!(plist.contains(required), "Info.plist is missing {required}");
    }
    assert!(!plist.contains("LSUIElement"));
    assert!(!plist.contains("LSBackgroundOnly"));
}

#[test]
fn windows_artifacts_are_self_contained_dpi_aware_and_inspected() {
    let cargo = source("Cargo.toml");
    let cargo_config = source(".cargo/config.toml");
    let build_script = source("build.rs");
    let manifest = source("packaging/windows/app.manifest");
    let verifier = source("scripts/verify-windows-artifacts.ps1");
    let ci = source(".github/workflows/ci.yml");
    let release = source(".github/workflows/deploy.yml");
    let local = source("scripts/deploy.sh");

    assert!(cargo.contains("embed-manifest = \"1.5.0\""));
    assert!(cargo.contains("embed-resource = \"3.0.11\""));
    assert!(cargo_config.contains("[target.x86_64-pc-windows-msvc]"));
    assert!(cargo_config.contains("target-feature=+crt-static"));
    assert!(build_script.contains("embed_manifest::embed_manifest_file"));
    assert!(build_script.contains("packaging/windows/app.manifest"));
    assert!(build_script.contains("embed_resource::compile_for"));
    assert!(build_script.contains("FileVersion"));
    assert!(build_script.contains("ProductVersion"));
    assert!(build_script.contains("Local Browser Bridge Server"));
    assert!(build_script.contains("Local Browser Bridge Computer Helper"));
    for required in [
        "level=\"asInvoker\"",
        "uiAccess=\"false\"",
        ">PerMonitorV2</dpiAwareness>",
        ">true</longPathAware>",
        "{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}",
    ] {
        assert!(
            manifest.contains(required),
            "manifest is missing {required}"
        );
        assert!(
            verifier.contains(required),
            "verifier is missing {required}"
        );
    }
    for required in [
        "dumpbin.exe",
        "mt.exe",
        "VCRUNTIME|MSVCP|api-ms-win-crt",
        "Assert-Manifest",
        "Assert-VersionResource",
        "Assert-StaticCrt",
        "FileVersion",
        "ProductVersion",
        "FileMajorPart",
        "ProductMajorPart",
        "OriginalFilename",
    ] {
        assert!(
            verifier.contains(required),
            "verifier is missing {required}"
        );
    }
    assert!(ci.contains("runs-on: windows-latest"));
    assert!(ci.contains("cargo test --locked --target x86_64-pc-windows-msvc --all-targets"));
    assert!(ci.contains(
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/verify-windows-artifacts.ps1 -Version 0.0.0 -SelfTest"
    ));
    assert!(ci.contains(
        "cargo clippy --locked --target x86_64-pc-windows-msvc --all-targets -- -D warnings"
    ));
    assert!(
        release.contains("cargo build --locked --release --bins --target x86_64-pc-windows-msvc")
    );
    assert!(release.contains("./scripts/verify-windows-artifacts.ps1 -Version $version"));
    assert!(local.contains("x86_64-pc-windows-msvc"));
    assert!(!local.contains("x86_64-pc-windows-gnu"));
}

#[test]
fn native_desktop_dependencies_are_not_built_on_unsupported_hosts() {
    let cargo = source("Cargo.toml");
    let target_section = cargo
        .split("[target.'cfg(any(target_os = \"macos\", target_os = \"windows\"))'.dependencies]")
        .nth(1)
        .unwrap()
        .split("[dev-dependencies]")
        .next()
        .unwrap();
    for dependency in ["image", "xcap"] {
        assert!(
            target_section.contains(dependency),
            "{dependency} must stay target-gated"
        );
    }
    assert!(!cargo.contains("enigo"));
    let unix_section = cargo
        .split("[target.'cfg(unix)'.dependencies]")
        .nth(1)
        .unwrap()
        .split("[target.'cfg(target_os = \"macos\")'.dependencies]")
        .next()
        .unwrap();
    assert!(
        unix_section.contains("libc"),
        "libc must stay Unix-gated for no-follow token-path validation"
    );
    let mac_section = cargo
        .split("[target.'cfg(target_os = \"macos\")'.dependencies]")
        .nth(1)
        .unwrap()
        .split("[dev-dependencies]")
        .next()
        .unwrap();
    for dependency in [
        "core-graphics",
        "foreign-types",
        "screencapturekit = { version = \"=8.0.1\", features = [\"macos_14_2\"] }",
    ] {
        assert!(
            mac_section.contains(dependency),
            "{dependency} must stay macOS-gated"
        );
    }
    let windows_section = cargo
        .split("[target.'cfg(target_os = \"windows\")'.dependencies]")
        .nth(1)
        .unwrap()
        .split("[dev-dependencies]")
        .next()
        .unwrap();
    assert!(
        windows_section.contains("windows-wgc = { package = \"windows\", version = \"0.62.2\"")
    );
    for feature in [
        "Graphics_Capture",
        "Win32_Graphics_Direct3D11",
        "Win32_System_WinRT_Graphics_Capture",
    ] {
        assert!(
            windows_section.contains(feature),
            "project-owned WGC is missing {feature}"
        );
    }
    assert!(!windows_section.contains("windows-capture"));

    let cargo_config = source(".cargo/config.toml");
    assert!(cargo_config.contains("MACOSX_DEPLOYMENT_TARGET"));
    assert!(cargo_config.contains("value = \"13.0\""));
    let build_script = source("build.rs");
    assert!(build_script.contains("xcrun"));
    assert!(build_script.contains("lib/swift/macosx"));

    let library = source("src/lib.rs");
    assert!(library.contains("cfg(any(target_os = \"macos\", target_os = \"windows\"))"));
    assert!(library.contains("path = \"computer_unsupported.rs\""));
    let unsupported = source("src/computer_unsupported.rs");
    assert!(unsupported.contains("COMPUTER_UNSUPPORTED_PLATFORM"));
    assert!(unsupported.contains("NATIVE_COMPUTER_SUPPORTED: bool = false"));
    for forbidden in ["image::", "xcap::", "platform_macos", "platform_windows"] {
        assert!(
            !unsupported.contains(forbidden),
            "unsupported-host stub must not reference {forbidden}"
        );
    }

    let ci = source(".github/workflows/ci.yml");
    assert!(ci.contains("runs-on: ubuntu-latest"));
    assert!(ci.contains("cargo clippy --locked --all-targets -- -D warnings"));
    assert!(ci.contains("cargo test --locked --all-targets"));
}

use std::fs;
#[cfg(not(target_os = "windows"))]
use std::process::Command;

fn source(path: &str) -> String {
    fs::read_to_string(path).unwrap().replace("\r\n", "\n")
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
fn release_workflow_and_local_builder_package_both_processes() {
    let workflow = source(".github/workflows/deploy.yml");
    let local = source("scripts/deploy.sh");
    for required in [
        "local-browser-bridge-v${version}-windows-x86_64.exe",
        "local-computer-helper-v${version}-windows-x86_64.exe",
        "local-browser-bridge-v${version}-macos-universal.tar.gz",
        "local-browser-bridge-extension-v${version}.zip",
        "Local Computer Helper.app/Contents/MacOS/local-computer-helper",
        "THIRD_PARTY_LICENSES.txt",
        "--licenses",
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
    assert!(workflow.contains("cargo build --locked --release --bins"));
    assert!(local.contains("cargo xwin build --locked --release --bins"));
    assert!(local.contains("release_stage=\"$(mktemp -d)\""));
    assert!(local.contains("validation_stage=\"$(mktemp -d)\""));
    assert!(
        local.contains(
            "pointer_handoff_self_test=\"$validation_stage/lbb-pointer-handoff-self-test\""
        )
    );
    assert!(
        !local
            .contains("pointer_handoff_self_test=\"$release_stage/lbb-pointer-handoff-self-test\"")
    );
    assert!(
        local.contains("bash scripts/verify-release-assets.sh \"$version\" \"$release_stage\"")
    );
    assert!(local.contains("cp \"$release_stage/$asset\" \"dist/$asset\""));
}

#[test]
fn windows_ci_and_release_validate_the_complete_browser_evidence_toolchain() {
    for path in [".github/workflows/ci.yml", ".github/workflows/deploy.yml"] {
        let workflow = source(path);
        for script in [
            "scripts/browser-evidence-candidate.ps1",
            "scripts/record-computer-helper-chain.ps1",
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
            "./scripts/sanitize-browser-evidence-screenshot.ps1 -Mode SelfTest",
            "./scripts/test-windows-browser-api.ps1 -SelfTest",
            "./scripts/test-windows-computer-use.ps1 -SelfTest",
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
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/sanitize-browser-evidence-screenshot.ps1 -Mode SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/test-windows-browser-api.ps1 -SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/write-browser-evidence-record.ps1 -Mode SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/write-stock-chrome-operator-response.ps1 -Mode SelfTest",
            "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/test-windows-computer-use.ps1 -SelfTest",
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
}

#[test]
fn release_runs_every_browser_evidence_self_test_under_windows_powershell_51() {
    let workflow = source(".github/workflows/deploy.yml");
    for invocation in [
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/browser-evidence-candidate.ps1 -Mode SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/record-computer-helper-chain.ps1 -Mode SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/sanitize-browser-evidence-screenshot.ps1 -Mode SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/test-windows-browser-api.ps1 -SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/write-browser-evidence-record.ps1 -Mode SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/write-stock-chrome-operator-response.ps1 -Mode SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/test-windows-computer-use.ps1 -SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/verify-windows-artifacts.ps1 -Version 0.0.0 -SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/test-windows-stock-chrome.ps1 -SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/verify-windows-release-candidate.ps1 -SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./scripts/wait-windows-foreground-arm-handoff.ps1 -Mode SelfTest",
        "& $windowsPowerShell -NoLogo -NoProfile -NonInteractive -File ./tests/fixtures/windows/WindowsComputerUseFixture.ps1 -SelfTest",
    ] {
        assert!(
            workflow.contains(invocation),
            "release validation does not run under Windows PowerShell 5.1: {invocation}"
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
fn macos_pointer_concurrency_handoff_watcher_is_read_only_and_release_gated() {
    let watcher = source("scripts/wait-macos-pointer-concurrency-handoff.mjs");
    let producer = source("evidence/v0.12.17/computer/helper-evidence-rig.mjs");
    let ci = source(".github/workflows/ci.yml");
    let release = source(".github/workflows/deploy.yml");
    let local = source("scripts/deploy.sh");

    for integration in [&ci, &release, &local] {
        assert!(
            integration.contains("node --check scripts/wait-macos-pointer-concurrency-handoff.mjs")
        );
        assert!(
            integration.contains(
                "node scripts/wait-macos-pointer-concurrency-handoff.mjs --mode self-test"
            )
        );
    }
    assert!(
        ci.matches("node --check scripts/wait-macos-pointer-concurrency-handoff.mjs")
            .count()
            >= 2
    );
    assert!(
        release
            .matches("node --check scripts/wait-macos-pointer-concurrency-handoff.mjs")
            .count()
            >= 2
    );

    for required in [
        "const PRODUCT_VERSION = \"0.12.17\";",
        "const SCHEMA_VERSION = 1;",
        "const OPERATOR_DIRECTORY = \"operator\";",
        "macos-pointer-concurrency-handoff-request.json",
        "macos-pointer-concurrency-handoff-complete.json",
        "macos-pointer-concurrency-handoff-request",
        "macos-pointer-concurrency-handoff-complete",
        "--mode watch --evidence-dir <absolute-path> --runner-pid <pid>",
        "open(path, constants.O_RDONLY | constants.O_NOFOLLOW)",
        "stats.mode & 0o077n",
        "stats.uid !== BigInt(process.getuid())",
        "process.kill(pid, 0)",
        "is stale.",
        "currentRequest.identity !== requestRecord.identity",
        "The request marker disappeared or changed after notification.",
        "does not match the request marker.",
        "The watched macOS acceptance runner is not alive.",
        "The nonactivating pointer prompt is not alive.",
        "ACTION REQUIRED: Continuously move the shared pointer without clicking; keep moving until COMPLETE.",
        "COMPLETE: Both boundaries observed sustained click-free shared-pointer movement.",
        "macOS pointer-concurrency handoff watcher self-test passed.",
    ] {
        assert!(
            watcher.contains(required),
            "macOS handoff watcher is missing {required}"
        );
    }

    for shared_contract in [
        "macos-pointer-concurrency-handoff-request.json",
        "macos-pointer-concurrency-handoff-complete.json",
        "macos-pointer-concurrency-handoff-request",
        "macos-pointer-concurrency-handoff-complete",
        "sustainedMotionSamples",
        "sustainedMotionSpanMilliseconds",
        "productBoundaryContaminated",
        "independentBoundaryContaminated",
        "clickFreeMotionObserved",
    ] {
        assert!(watcher.contains(shared_contract));
        assert!(
            producer.contains(shared_contract),
            "macOS handoff producer and watcher disagree on {shared_contract}"
        );
    }
    assert!(producer.contains("const POINTER_HANDOFF_MARKER_SCHEMA = 1;"));
    assert!(producer.contains("const operatorDirectory = join(outputDir, \"operator\");"));
    assert!(watcher.contains("join(evidenceDir, OPERATOR_DIRECTORY)"));

    for integration in [&ci, &release, &local] {
        assert!(
            integration.contains("node --check evidence/v0.12.17/computer/helper-evidence-rig.mjs"),
            "release path does not syntax-check the exact macOS evidence rig"
        );
        assert!(
            integration
                .contains("node evidence/v0.12.17/computer/helper-evidence-rig.mjs --self-test")
        );
    }
    for integration in [&ci, &release] {
        for source in [
            "HelperEvidenceFixture.swift",
            "SystemProbe.swift",
            "PointerHandoff.swift",
        ] {
            assert!(
                integration.contains(&format!(
                    "xcrun swiftc -typecheck evidence/v0.12.17/computer/{source}"
                )),
                "macOS workflow does not typecheck {source}"
            );
        }
        assert!(integration.contains("lbb-pointer-handoff-self-test\" --self-test"));
    }

    for field in [
        "schemaVersion",
        "kind",
        "productVersion",
        "requestId",
        "createdAt",
        "runnerPid",
        "promptPid",
        "requestDelivered",
        "panelOnScreen",
        "panelNonactivating",
        "notificationOnly",
        "acceptedAsAuthority",
        "sustainedMotionSamples",
        "sustainedMotionSpanMilliseconds",
        "productBoundaryContaminated",
        "independentBoundaryContaminated",
        "clickFreeMotionObserved",
    ] {
        assert!(
            watcher.contains(&format!("\"{field}\"")),
            "macOS handoff watcher omits schema field {field}"
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
        "notificationOnly: false",
        "--ack",
    ] {
        assert!(
            !watcher.contains(forbidden),
            "read-only macOS handoff watcher contains forbidden primitive: {forbidden}"
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
    assert!(!lockfile.contains("name = \"option-ext\""));
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
fn release_gates_javascript_macos_and_published_provenance() {
    let ci = source(".github/workflows/ci.yml");
    let release = source(".github/workflows/deploy.yml");
    let verifier = source("scripts/verify-release-assets.sh");
    let node_pin = "actions/setup-node@249970729cb0ef3589644e2896645e5dc5ba9c38 # v6.5.0";

    assert!(ci.matches(node_pin).count() >= 4);
    assert!(!ci.contains("Extension static and contract tests (Node-free)"));
    assert!(ci.contains("name: macOS formatting, lint, and tests"));
    assert_eq!(ci.matches("runs-on: macos-26").count(), 1);
    assert_eq!(release.matches("runs-on: macos-26").count(), 1);
    assert!(!ci.contains("runs-on: macos-14"));
    assert!(!release.contains("runs-on: macos-14"));
    assert!(ci.contains("bash scripts/verify-macos-build-host.sh"));
    assert!(release.contains("bash scripts/verify-macos-build-host.sh"));
    let macos_host_verifier = source("scripts/verify-macos-build-host.sh");
    assert!(macos_host_verifier.contains("required_sdk_major=26"));
    assert!(macos_host_verifier.contains("required_deployment_target=\"13.0\""));
    assert!(macos_host_verifier.contains("xcrun --sdk macosx --show-sdk-version"));
    assert!(!release.contains("workflow_dispatch:"));
    assert!(release.contains("RELEASE_TAG: ${{ github.ref_name }}"));

    for required in [
        "Run native macOS formatting, lint, and tests",
        "^v[0-9]+\\.[0-9]+\\.[0-9]+$",
        "Assemble frozen release candidate",
        "Freeze the exact candidate for interactive acceptance",
        "name: release-candidate",
        "retention-days: 14",
        "environment:\n      name: release",
        "Re-verify the frozen candidate and every build attestation",
        "Refuse publication unless release immutability is enabled",
        "Recover a verified draft or publish immutable release assets",
        "assert_release_identity \"$created_draft\" true false",
        "assert_release_assets \"$created_draft\" exact",
        "download_and_compare_release_assets \"$created_draft\"",
        "cmp -s \"dist/$asset_name\" \"$download_dir/$asset_name\"",
        "repos/$GITHUB_REPOSITORY/immutable-releases",
        "X-GitHub-Api-Version: 2026-03-10",
        "--jq '.enabled'",
        "--source-ref \"$GITHUB_REF\"",
        "VERIFIED_SOURCE_SHA: ${{ needs.verify.outputs.source_sha }}",
        "--source-digest \"$VERIFIED_SOURCE_SHA\"",
        "--signer-workflow \"$GITHUB_REPOSITORY/.github/workflows/deploy.yml\"",
        "--deny-self-hosted-runners",
        "Re-download and verify the immutable published release",
        "gh release download \"$RELEASE_TAG\"",
        "gh release verify \"$RELEASE_TAG\"",
        "gh release verify \"$RELEASE_TAG\" --repo \"$GITHUB_REPOSITORY\" --format json",
        "gh release verify-asset \"$RELEASE_TAG\"",
        "expected_purl=\"pkg:github/${GITHUB_REPOSITORY}@${RELEASE_TAG}\"",
        ".digest.sha1",
        "test \"$release_tag_sha\" = \"$VERIFIED_TAG_SHA\"",
        "bash scripts/verify-release-assets.sh \"$version\" published",
        "gh release verify-asset $RELEASE_TAG <file>",
        "--source-ref refs/tags/$RELEASE_TAG",
    ] {
        assert!(
            release.contains(required),
            "release gate is missing {required}"
        );
    }

    assert!(
        verifier
            .contains("Release checksum manifest is missing, empty, linked, or not a regular file")
    );
    assert!(verifier.contains("Release asset is missing, empty, linked, or not a regular file"));
    assert!(verifier.contains("Release directory contains an unexpected file set."));
    assert!(verifier.contains("while IFS= read -r line || [[ -n \"$line\" ]]; do"));
    assert!(verifier.contains("checksum_lines+=(\"$line\")"));
    assert!(verifier.contains("Checksum manifest must contain exactly four canonical lines."));
    assert!(verifier.contains("[[ ! \"$hash\" =~ ^[0-9a-f]{64}$ ]]"));
    assert!(verifier.contains("[[ \"${line:64:2}\" != \"  \" ]]"));
    assert!(verifier.contains("[[ \"${line:66}\" != \"${expected_checksum_files[$index]}\" ]]"));
    assert!(verifier.contains("cmp -s \"$canonical_checksum_manifest\" \"$checksum_manifest\""));
    assert!(verifier.contains("Checksum manifest bytes are not canonical LF-terminated ASCII."));
    assert!(!verifier.contains("if [[ -f \"$checksum_manifest\" ]]"));

    let immutable_gate = release
        .find("Refuse publication unless release immutability is enabled")
        .unwrap();
    let exact_asset_gate = release
        .find("bash scripts/verify-release-assets.sh \"$version\" dist")
        .unwrap();
    let publish = release.find("gh release create \"$RELEASE_TAG\"").unwrap();
    assert!(immutable_gate < publish);
    assert!(exact_asset_gate < publish);
    assert_eq!(
        release
            .matches("(cd dist && sha256sum \"${assets[@]}\" > SHA256SUMS.txt)")
            .count(),
        1,
        "the checksum manifest must be generated once in the frozen candidate, never rebuilt after acceptance"
    );
    let freeze = release
        .find("Freeze the exact candidate for interactive acceptance")
        .unwrap();
    let approval = release.find("environment:\n      name: release").unwrap();
    assert!(freeze < approval);
    assert!(approval < publish);
    let publish_draft = release.find("-F draft=false").unwrap();
    let upload = release
        .find("gh release upload \"$RELEASE_TAG\" dist/*")
        .unwrap();
    let draft_identity = release
        .find("assert_release_identity \"$created_draft\" true false")
        .unwrap();
    let draft_asset_set = release
        .find("assert_release_assets \"$created_draft\" exact")
        .unwrap();
    let draft_byte_comparison = release
        .find("download_and_compare_release_assets \"$created_draft\"")
        .unwrap();
    assert!(publish < upload);
    assert!(upload < draft_identity);
    assert!(draft_identity < draft_asset_set);
    assert!(draft_asset_set < draft_byte_comparison);
    assert!(draft_byte_comparison < publish_draft);
    let final_tag_recheck = release[..publish_draft].rfind("assert_remote_tag").unwrap();
    let final_immutable_recheck = release[..publish_draft]
        .rfind("\"repos/$GITHUB_REPOSITORY/immutable-releases\"")
        .unwrap();
    assert!(publish < final_tag_recheck);
    assert!(final_tag_recheck < final_immutable_recheck);
    assert!(final_immutable_recheck < publish_draft);
    assert_eq!(
        release
            .matches("\"repos/$GITHUB_REPOSITORY/immutable-releases\"")
            .count(),
        2,
        "release immutability must be checked both before approval and immediately before draft publication"
    );
    let final_publication_gate = &release[final_tag_recheck..publish_draft];
    assert!(final_publication_gate.contains("X-GitHub-Api-Version: 2026-03-10"));
    assert!(final_publication_gate.contains("--jq '.enabled')\" = true"));
    let published_attestation = release
        .find("release_verification=\"$(gh release verify")
        .unwrap();
    let tag_subject_check = release
        .find("test \"$release_tag_sha\" = \"$VERIFIED_TAG_SHA\"")
        .unwrap();
    assert!(publish_draft < published_attestation);
    assert!(published_attestation < tag_subject_check);
    let published_verification_step = release
        .split("- name: Re-download and verify the immutable published release")
        .nth(1)
        .unwrap()
        .split("shell: bash")
        .next()
        .unwrap();
    assert!(
        published_verification_step
            .contains("VERIFIED_TAG_SHA: ${{ needs.verify.outputs.tag_sha }}")
    );
}

#[test]
fn release_requires_canonical_schema_two_receipt_and_committed_evidence() {
    let release = source(".github/workflows/deploy.yml");
    let verifier = source("scripts/verify-release-acceptance-evidence.sh");
    let receipt_gate = release
        .split("- name: Require exact candidate-bound committed acceptance evidence")
        .nth(1)
        .unwrap()
        .split("- name: Recover a verified draft or publish immutable release assets")
        .next()
        .unwrap();

    for required in [
        "actions: read # Re-downloads and raw-hashes the exact frozen workflow artifact.",
        "LBB_RELEASE_ACCEPTANCE_V2: ${{ vars.LBB_RELEASE_ACCEPTANCE_V2 }}",
        "test -n \"${LBB_RELEASE_ACCEPTANCE_V2:-}\"",
        "test \"${#LBB_RELEASE_ACCEPTANCE_V2}\" -le 4096",
        "printf '%s' \"$LBB_RELEASE_ACCEPTANCE_V2\" > \"$receipt_file\"",
        "bash scripts/verify-release-acceptance-evidence.sh \"$receipt_file\" dist",
        "acceptance_receipt_sha256=\"$(sha256sum \"$receipt_file\"",
        "printf 'LBB_RELEASE_ACCEPTANCE_RECEIPT_SHA256=%s\\n'",
        ">> \"$GITHUB_ENV\"",
        "GH_TOKEN: ${{ github.token }}",
        "VERIFIED_SOURCE_SHA: ${{ needs.verify.outputs.source_sha }}",
        "VERIFIED_TAG_SHA: ${{ needs.verify.outputs.tag_sha }}",
    ] {
        assert!(
            release.contains(required),
            "release acceptance evidence gate is missing {required}"
        );
    }
    assert!(
        release.contains("run: bash scripts/verify-release-acceptance-evidence.sh --self-test")
    );
    assert!(!release.contains("LBB_RELEASE_ACCEPTANCE_V1"));
    assert!(!release.contains("local-browser-bridge-release:v1"));
    assert!(!receipt_gate.contains("secrets.LBB_RELEASE_ACCEPTANCE_V2"));

    for key in [
        "schemaVersion",
        "tag",
        "sourceSha",
        "tagObjectSha",
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
    ] {
        assert!(
            verifier.contains(&format!("\"{key}\"")),
            "canonical schema-2 receipt omits {key}"
        );
    }
    for required in [
        "keys_unsorted == [",
        ".schemaVersion == 2",
        ".tag == $tag",
        ".sourceSha == $source_sha",
        ".tagObjectSha == $tag_object_sha",
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
    ] {
        assert!(
            verifier.contains(required),
            "schema-2 verifier is missing {required}"
        );
    }

    let freeze = release
        .find("Freeze the exact candidate for interactive acceptance")
        .unwrap();
    let receipt = release
        .find("Require exact candidate-bound committed acceptance evidence")
        .unwrap();
    let release_mutation = release
        .find("Recover a verified draft or publish immutable release assets")
        .unwrap();
    assert!(freeze < receipt);
    assert!(receipt < release_mutation);
    assert!(receipt < release.find("gh release create \"$RELEASE_TAG\"").unwrap());

    let publication = &release[release_mutation..];
    for required in [
        "[[ \"${LBB_RELEASE_ACCEPTANCE_RECEIPT_SHA256:-}\" =~ ^[0-9a-f]{64}$ ]]",
        "acceptance-receipt-sha256=$LBB_RELEASE_ACCEPTANCE_RECEIPT_SHA256",
        "Acceptance receipt SHA-256: \\`$LBB_RELEASE_ACCEPTANCE_RECEIPT_SHA256\\`",
        "cmp -s release-notes.md \"$release_body\"",
    ] {
        assert!(
            publication.contains(required),
            "published release does not bind the accepted receipt: {required}"
        );
    }
    for forbidden in [
        "release_acceptance_receipt_sha256()",
        "existing_acceptance_receipt_sha256=\"$(release_acceptance_receipt_sha256",
        "sed \"s/$LBB_RELEASE_ACCEPTANCE_RECEIPT_SHA256/$expected_acceptance_receipt_sha256/g\"",
    ] {
        assert!(
            !publication.contains(forbidden),
            "release recovery must not trust a receipt hash embedded by an existing release: {forbidden}"
        );
    }
}

#[test]
fn release_evidence_verifier_fails_closed_on_artifact_or_commit_substitution() {
    let verifier = source("scripts/verify-release-acceptance-evidence.sh");
    for required in [
        "actions/runs/$GITHUB_RUN_ID/attempts/$GITHUB_RUN_ATTEMPT",
        "actions/runs/$GITHUB_RUN_ID/attempts/$GITHUB_RUN_ATTEMPT/jobs?per_page=100",
        "actions/runs/$GITHUB_RUN_ID/artifacts?per_page=100",
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
        ".pointerEvidence.concurrentSharedSeatActivityObserved == true",
        ".quietSeatStabilization.required == true",
        ".quietSeatStabilization.completed == true",
        ".quietSeatStabilization.completedBeforeCandidateExecution == true",
        ".quietSeatStabilization.stableDurationMilliseconds >= 30000",
        ".quietSeatStabilization.observedSamples >= 61",
        ".quietSeatStabilization.stableTransitions >= 60",
        ".quietSeatStabilization.monitoringUnknown == false",
        ".operatorHandoff.clickFreeMotionObserved == true",
        ".operatorHandoff.productBoundaryContaminated == true",
        ".operatorHandoff.independentBoundaryContaminated == true",
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
fn release_reruns_delete_only_a_candidate_bound_byte_exact_draft() {
    let release = source(".github/workflows/deploy.yml");

    for binding in [
        "local-browser-bridge-release:v2",
        "source-sha=$VERIFIED_SOURCE_SHA",
        "tag-object-sha=$VERIFIED_TAG_SHA",
        "manifest-sha256=$candidate_manifest_sha256",
        "acceptance-receipt-sha256=$LBB_RELEASE_ACCEPTANCE_RECEIPT_SHA256",
        "jq -ej '.body | select(type == \"string\")'",
        "cmp -s release-notes.md \"$release_body\"",
    ] {
        assert!(
            release.contains(binding),
            "recoverable draft ownership is missing {binding}"
        );
    }
    assert!(!release.contains("jq -ejr '.body | select(type == \"string\")'"));
    assert!(
        release
            .contains("https://api.github.com/repos/$GITHUB_REPOSITORY/releases/assets/$asset_id")
    );
    assert!(release.contains("[[ \"$asset_id\" =~ ^[1-9][0-9]*$ ]]"));
    assert!(!release.contains("((.id | type) == \"number\")"));

    let draft_start = release
        .find("if jq -e '.isDraft == true and .isImmutable == false'")
        .unwrap();
    let published_start = release
        .find("elif jq -e '.isDraft == false and .isImmutable == true'")
        .unwrap();
    let draft = &release[draft_start..published_start];
    let identity = draft
        .find("assert_release_identity \"$existing_release\" true false")
        .unwrap();
    let subset = draft
        .find("assert_release_assets \"$existing_release\" subset")
        .unwrap();
    let byte_comparison = draft
        .find("download_and_compare_release_assets \"$existing_release\"")
        .unwrap();
    let stable_identity = draft
        .find("test \"$(release_fingerprint \"$refreshed_draft\")\" = \"$draft_fingerprint\"")
        .unwrap();
    let delete = draft.find("--method DELETE").unwrap();
    let absent = draft.find("wait_for_release_absence").unwrap();
    assert!(identity < subset);
    assert!(subset < byte_comparison);
    assert!(byte_comparison < stable_identity);
    assert!(stable_identity < delete);
    assert!(delete < absent);
    assert!(draft.contains("repos/$GITHUB_REPOSITORY/releases/$draft_release_id"));
    assert!(draft.matches("assert_remote_tag").count() >= 2);
    assert_eq!(release.matches("--method DELETE").count(), 1);
    assert!(!release.contains("gh release delete"));
    assert!(!release.contains("--cleanup-tag"));
}

#[test]
fn release_reruns_resume_only_an_exact_immutable_publication() {
    let release = source(".github/workflows/deploy.yml");
    let published_start = release
        .find("elif jq -e '.isDraft == false and .isImmutable == true'")
        .unwrap();
    let rejected_start = release[published_start..]
        .find("else\n              echo \"The existing release is neither")
        .map(|offset| published_start + offset)
        .unwrap();
    let published = &release[published_start..rejected_start];

    let identity = published
        .find("assert_release_identity \"$existing_release\" false true")
        .unwrap();
    let exact_assets = published
        .find("assert_release_assets \"$existing_release\" exact")
        .unwrap();
    let byte_comparison = published
        .find("download_and_compare_release_assets \"$existing_release\"")
        .unwrap();
    let stable_identity = published
        .find(
            "test \"$(release_fingerprint \"$refreshed_published\")\" = \"$published_fingerprint\"",
        )
        .unwrap();
    let resume = published.find("exit 0").unwrap();
    assert!(identity < exact_assets);
    assert!(exact_assets < byte_comparison);
    assert!(byte_comparison < stable_identity);
    assert!(stable_identity < resume);
    assert!(published.contains("assert_remote_tag"));
    assert!(!published.contains("gh release upload"));
    assert!(!published.contains("--method DELETE"));

    let rejected = &release[rejected_start..release.find("gh release create").unwrap()];
    assert!(rejected.contains("neither a recoverable draft nor an immutable publication"));
    assert!(rejected.contains("exit 1"));
    assert!(release.contains("wait_for_release_absence()"));
    assert!(release.contains("for attempt in 1 2 3 4 5 6 7 8 9 10; do"));
    assert!(release.contains("publication_visible=false"));
    assert!(release.contains("test \"$publication_visible\" = true"));

    let published_verification = release
        .split("- name: Re-download and verify the immutable published release")
        .nth(1)
        .unwrap();
    assert!(published_verification.contains("bash scripts/verify-release-assets.sh"));
    assert!(published_verification.contains("cmp -s \"$subject\" \"published/$asset\""));
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
    for path in [".github/workflows/ci.yml", ".github/workflows/deploy.yml"] {
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

    let release = source(".github/workflows/deploy.yml");
    for forbidden in [
        "--source-digest \"${{",
        "= \"${{ needs.verify.outputs.tag_sha }}\"",
        "= \"${{ needs.verify.outputs.version }}\"",
    ] {
        assert!(
            !release.contains(forbidden),
            "release shell code contains direct template expansion: {forbidden}"
        );
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
    assert!(ci.contains("./scripts/verify-windows-artifacts.ps1 -Version $version"));
    assert!(release.contains(
        "cargo clippy --locked --target x86_64-pc-windows-msvc --all-targets -- -D warnings"
    ));
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

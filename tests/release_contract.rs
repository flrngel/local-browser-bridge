use std::fs;

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
        local.contains("xcrun swiftc -typecheck tests/fixtures/macos/CiAcceptanceFixture.swift")
    );
    assert!(local.contains("node --check scripts/ci-acceptance.mjs"));
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
fn current_source_is_unblocked_and_package_versions_are_aligned() {
    match fs::symlink_metadata("RELEASE_BLOCKED") {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => panic!("could not inspect the release-blocker path: {error}"),
        Ok(_) => {
            panic!("the reviewed source must not retain a release-blocker file or symlink")
        }
    }

    let version = source("Cargo.toml")
        .lines()
        .find_map(|line| {
            line.strip_prefix("version = \"")
                .and_then(|rest| rest.strip_suffix('"'))
                .map(str::to_owned)
        })
        .expect("Cargo.toml package version");
    assert!(
        version.split('.').count() == 3
            && version.split('.').all(|part| part.parse::<u64>().is_ok()),
        "package version must be a release version: {version}"
    );

    // Every pinned location is declared once and shared by the audit and the bump.
    let pins = source("scripts/version-pins.txt");
    let mut pinned_files = Vec::new();
    for line in pins
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let fields: Vec<&str> = line.split('|').collect();
        assert_eq!(fields.len(), 4, "malformed version pin: {line}");
        let (file, prefix, suffix, scope) = (fields[0], fields[1], fields[2], fields[3]);
        assert!(
            matches!(scope, "line" | "any"),
            "unknown pin scope in {line}"
        );
        let literal = format!("{prefix}{version}{suffix}");
        let content = source(file);
        let found = if scope == "line" {
            content
                .lines()
                .any(|candidate| candidate.starts_with(&literal))
        } else {
            content.contains(&literal)
        };
        assert!(found, "{file} does not pin the package version: {literal}");
        pinned_files.push(file.to_owned());
    }
    for required in [
        "Cargo.toml",
        "extension/manifest.json",
        "extension/lib.js",
        ".github/workflows/deploy.yml",
        ".github/workflows/publish.yml",
        "docs/DEVELOPMENT.md",
    ] {
        assert!(
            pinned_files.iter().any(|file| file == required),
            "{required} must be a declared version pin"
        );
    }
    assert!(source("Cargo.lock").contains(&format!(
        "name = \"local-browser-bridge\"\nversion = \"{version}\""
    )));

    let audit = source("scripts/audit-versions.sh");
    let bump = source("scripts/bump-version.sh");
    for script in [&audit, &bump] {
        assert!(script.contains("scripts/version-pins.txt"));
    }
    assert!(bump.contains("rewrite_lock"));
    assert!(bump.contains("cargo metadata --locked --offline --format-version 1"));
    assert!(bump.contains("bash scripts/audit-versions.sh \"$new_version\""));
    assert!(
        source(".github/workflows/ci.yml").contains("bash scripts/bump-version.sh --self-test")
    );
    assert!(source("docs/DEVELOPMENT.md").contains("bash scripts/bump-version.sh"));

    // The retired operator harness must not come back as a pinned consumer.
    for retired in [
        "scripts/run-windows-computer-use-acceptance.ps1",
        "scripts/verify-release-acceptance-evidence.sh",
        "scripts/finalize-macos-acceptance.mjs",
        "scripts/verify-windows-release-candidate.ps1",
    ] {
        assert!(
            !std::path::Path::new(retired).exists(),
            "{retired} was retired with the operator harness"
        );
    }
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
    assert!(publication.contains(
        "release:\n    name: Publish exact accepted release\n    needs: preflight\n    if: ${{ !inputs.dry_run }}"
    ));
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
    assert_eq!(
        workflow_job_ids(&ci),
        vec!["rust", "windows", "macos", "acceptance"]
    );
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
        vec![
            "verify",
            "windows",
            "macos",
            "assemble",
            "acceptance",
            "receipt"
        ]
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
        5,
        "extension, Windows, macOS, checksum manifest, and acceptance receipt must each be attested"
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
        2,
        "the frozen candidate and its acceptance receipt share one retention window"
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
    assert!(preflight.contains(".name == \"acceptance-receipt\" and .expired == false"));
    assert!(preflight.contains(".schemaVersion == 4"));
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
        windows_section.contains("windows = { version = \"0.62.2\""),
        "the windows crate must stay a single unified dependency"
    );
    assert!(
        !windows_section.contains("package = \"windows\""),
        "the windows crate must not be depended on twice under different names"
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

#[test]
fn ci_hosted_acceptance_replaces_the_operator_harness_everywhere() {
    let acceptance = source(".github/workflows/acceptance.yml");
    let ci = source(".github/workflows/ci.yml");
    let candidate = source(".github/workflows/deploy.yml");
    let publication = source(".github/workflows/publish.yml");
    let driver = source("scripts/ci-acceptance.mjs");

    // One reusable workflow serves both the PR source lane and the frozen-candidate lane.
    assert!(acceptance.contains("workflow_call:"));
    for input in [
        "mode:",
        "candidate_run_id:",
        "candidate_run_attempt:",
        "version:",
        "source_sha:",
    ] {
        assert!(
            acceptance.contains(input),
            "acceptance.yml is missing input {input}"
        );
    }
    assert!(acceptance.contains("- os: windows-latest"));
    assert!(acceptance.contains("- os: macos-26"));
    assert!(acceptance.contains("fail-fast: false"));
    assert!(acceptance.contains("CHROME_FOR_TESTING_BUILD:"));
    assert!(acceptance.contains("npx --yes @puppeteer/browsers@"));
    assert!(acceptance.contains("node scripts/ci-acceptance.mjs"));
    assert!(acceptance.contains(".name == \"release-candidate\" and .expired == false"));
    assert!(acceptance.contains("SHA256SUMS.txt"));
    assert!(acceptance.contains("name: acceptance-${{ matrix.label }}"));
    assert!(acceptance.contains("retention-days: 30"));
    assert!(acceptance.contains("if: always()"));
    assert!(!acceptance.contains("continue-on-error:"));

    let ci_acceptance = job_section(&ci, "acceptance");
    assert!(ci_acceptance.contains("uses: ./.github/workflows/acceptance.yml"));
    assert!(ci_acceptance.contains("mode: source"));

    let candidate_acceptance = job_section(&candidate, "acceptance");
    assert!(candidate_acceptance.contains("needs: [verify, assemble]"));
    assert!(candidate_acceptance.contains("uses: ./.github/workflows/acceptance.yml"));
    assert!(candidate_acceptance.contains("mode: artifact"));
    assert!(candidate_acceptance.contains("candidate_run_id: ${{ github.run_id }}"));
    assert!(candidate_acceptance.contains("candidate_run_attempt: ${{ github.run_attempt }}"));
    let receipt = job_section(&candidate, "receipt");
    assert!(receipt.contains("needs: [verify, assemble, acceptance]"));
    assert!(receipt.contains("schemaVersion: 4"));
    assert!(receipt.contains("candidateArtifactSha256: $zip_sha"));
    assert!(receipt.contains("all(.checks[]; (.required | not) or .status == \"pass\")"));
    assert!(receipt.contains("subject-path: acceptance-receipt.json"));
    assert!(receipt.contains("name: acceptance-receipt"));

    // Publication verifies the attested receipt instead of an operator-typed one.
    assert!(!publication.contains("acceptance_receipt:"));
    assert!(publication.contains("dry_run:"));
    assert!(publication.contains("type: boolean"));
    let preflight = job_section(&publication, "preflight");
    assert!(preflight.contains("gh attestation verify \"$receipt\""));
    assert!(
        preflight.contains("--signer-workflow \"$GITHUB_REPOSITORY/.github/workflows/deploy.yml\"")
    );
    assert!(preflight.contains(".candidateArtifactSha256 == $zip_sha"));
    assert!(preflight.contains("(.results | keys) == [\"macos\", \"windows\"]"));
    assert!(preflight.contains("all(.checks[]; (.required | not) or .status == \"pass\")"));
    assert!(preflight.contains("bash scripts/publish-release.sh check-remote"));

    // The driver records every check in the shared schema and fails closed.
    for required in [
        "schemaVersion: 1",
        "status === \"pass\"",
        "\"skip\"",
        "permission-unavailable",
        "COMPUTER_CAPTURE_FAILED",
        "COMPUTER_INPUT_FAILED",
        "SHELL_DISABLED",
        "--load-extension=",
        "--use-mock-keychain",
        "--password-store=basic",
        "chrome.storage.local.set(",
        "browser.control.start",
        "browser.control.stop",
        "computer.share.start",
        "computer.typeText",
        "process.exit(ok ? 0 : 1)",
    ] {
        assert!(
            driver.contains(required),
            "ci-acceptance.mjs is missing {required}"
        );
    }
    assert!(
        driver
            .contains("const gated = IS_MACOS && !(captureReady && semanticReady && inputReady);")
    );
    assert!(
        !driver.contains("require("),
        "the driver must stay dependency-free ESM"
    );
    assert!(!driver.contains("node_modules"));
}

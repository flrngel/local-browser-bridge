use std::fs;

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
    assert!(
        local.contains("bash scripts/verify-release-assets.sh \"$version\" \"$release_stage\"")
    );
    assert!(local.contains("cp \"$release_stage/$asset\" \"dist/$asset\""));
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
        "unzip -p \"$extension_archive\" LICENSE",
        "cmp -s \"$mac_stage/$notice\" \"$notice\"",
        "THIRD_PARTY_LICENSES.txt",
        "--licenses",
    ] {
        assert!(
            verifier.contains(required),
            "release verifier is missing license check: {required}"
        );
    }
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
    assert!(ci.contains("runs-on: macos-14"));
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
        "gh release verify-asset \"$RELEASE_TAG\"",
        "bash scripts/verify-release-assets.sh \"$version\" published",
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
        "libc",
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

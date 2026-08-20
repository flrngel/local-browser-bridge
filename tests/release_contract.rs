use std::fs;

#[test]
fn release_workflow_and_local_builder_package_both_processes() {
    let workflow = fs::read_to_string(".github/workflows/deploy.yml").unwrap();
    let local = fs::read_to_string("scripts/deploy.sh").unwrap();
    for required in [
        "local-browser-bridge-v${version}-windows-x86_64.exe",
        "local-computer-helper-v${version}-windows-x86_64.exe",
        "local-browser-bridge-v${version}-macos-universal.tar.gz",
        "local-browser-bridge-extension-v${version}.zip",
        "Local Computer Helper.app/Contents/MacOS/local-computer-helper",
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
}

#[test]
fn mac_helper_bundle_has_a_stable_visible_identity() {
    let plist = fs::read_to_string("packaging/macos/Info.plist.in").unwrap();
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
fn native_desktop_dependencies_are_not_built_on_unsupported_hosts() {
    let cargo = fs::read_to_string("Cargo.toml").unwrap();
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
    assert!(windows_section.contains("windows-capture = \"=2.0.1\""));

    let cargo_config = fs::read_to_string(".cargo/config.toml").unwrap();
    assert!(cargo_config.contains("MACOSX_DEPLOYMENT_TARGET"));
    assert!(cargo_config.contains("value = \"13.0\""));
    let build_script = fs::read_to_string("build.rs").unwrap();
    assert!(build_script.contains("xcrun"));
    assert!(build_script.contains("lib/swift/macosx"));

    let library = fs::read_to_string("src/lib.rs").unwrap();
    assert!(library.contains("cfg(any(target_os = \"macos\", target_os = \"windows\"))"));
    assert!(library.contains("path = \"computer_unsupported.rs\""));
    let unsupported = fs::read_to_string("src/computer_unsupported.rs").unwrap();
    assert!(unsupported.contains("COMPUTER_UNSUPPORTED_PLATFORM"));
    assert!(unsupported.contains("NATIVE_COMPUTER_SUPPORTED: bool = false"));
    for forbidden in ["image::", "xcap::", "platform_macos", "platform_windows"] {
        assert!(
            !unsupported.contains(forbidden),
            "unsupported-host stub must not reference {forbidden}"
        );
    }

    let ci = fs::read_to_string(".github/workflows/ci.yml").unwrap();
    assert!(ci.contains("runs-on: ubuntu-latest"));
    assert!(ci.contains("cargo clippy --locked --all-targets -- -D warnings"));
    assert!(ci.contains("cargo test --locked --all-targets"));
}

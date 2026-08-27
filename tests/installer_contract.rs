use std::fs;

fn source(path: &str) -> String {
    fs::read_to_string(path).unwrap_or_else(|error| panic!("could not read {path}: {error}"))
}

#[test]
fn installers_fail_closed_and_keep_desktop_authority_opt_in() {
    let windows = source("scripts/install-windows.ps1");
    let macos = source("scripts/install-macos.sh");

    for required in [
        "immutable",
        "prerelease",
        "sha256:[0-9a-f]{64}",
        "SHA256SUMS.txt",
        "Unexpected release asset inventory",
        "ReparsePoint",
        "Assert-SafeInstallRoot",
        "[switch]$StartHelper",
        "if ($StartHelper)",
    ] {
        assert!(
            windows.contains(required),
            "Windows installer is missing `{required}`"
        );
    }
    for required in [
        "obj.immutable !== true",
        "obj.prerelease",
        "^sha256:[0-9a-f]{64}$",
        "SHA256SUMS.txt",
        "unexpected asset count",
        "Refusing a symlink install path",
        "assert_safe_install_root",
        "start_helper=0",
        "if ((start_helper))",
    ] {
        assert!(
            macos.contains(required),
            "macOS installer is missing `{required}`"
        );
    }
    assert!(!windows.contains("Set-MpPreference"));
    assert!(!windows.contains("ExecutionPolicy Bypass"));
    assert!(!macos.contains("spctl --master-disable"));
    assert!(!macos.contains("xattr -dr com.apple.quarantine"));
}

#[test]
fn installer_assets_match_the_update_contract() {
    let update = source("src/update.rs");
    let windows = source("scripts/install-windows.ps1");
    let macos = source("scripts/install-macos.sh");
    let versioned_stems = [
        "local-browser-bridge-extension-v",
        "local-browser-bridge-v",
        "local-computer-helper-v",
    ];
    for stem in versioned_stems {
        assert!(update.contains(stem));
        assert!(windows.contains(stem));
        assert!(macos.contains(stem));
    }
    assert!(windows.contains("$assets.Count -ne $expected.Count"));
    assert!(macos.contains("obj.assets.length !== expected.length"));
}

#[test]
fn user_docs_lead_with_one_command_installation() {
    let readme = source("README.md");
    let windows = source("docs/INSTALL_WINDOWS.md");
    let macos = source("docs/INSTALL_MACOS.md");
    assert!(readme.contains("## Install in one command"));
    assert!(windows.contains("## One-command install"));
    assert!(windows.contains("scripts/install-windows.ps1"));
    assert!(macos.contains("## One-command install"));
    assert!(macos.contains("scripts/install-macos.sh | bash"));
    assert!(windows.contains("Load unpacked"));
    assert!(macos.contains("Load unpacked"));
}

#[test]
fn existing_ci_jobs_self_test_the_installers() {
    let ci = source(".github/workflows/ci.yml");
    assert!(ci.contains("bash scripts/install-macos.sh --self-test"));
    assert!(ci.contains("\"scripts/install-windows.ps1\""));
    assert!(ci.contains("-File ./scripts/install-windows.ps1 -SelfTest"));
}

#[test]
fn installers_make_extension_setup_and_later_launch_discoverable() {
    let windows = source("scripts/install-windows.ps1");
    let macos = source("scripts/install-macos.sh");
    let windows_docs = source("docs/INSTALL_WINDOWS.md");
    let macos_docs = source("docs/INSTALL_MACOS.md");

    for required in [
        "Show-ExtensionSetup",
        "System32\\clip.exe",
        "Finish Local Browser Bridge Setup",
        "Finish Browser Extension Setup.cmd",
        "Open Local Browser Bridge.cmd",
        "Start Computer Helper.cmd",
        "[Environment]::GetFolderPath(\"Programs\")",
    ] {
        assert!(
            windows.contains(required),
            "Windows guided setup is missing `{required}`"
        );
    }
    for required in [
        "show_extension_setup",
        "/usr/bin/pbcopy",
        "Finish Local Browser Bridge Setup",
        "Finish Browser Extension Setup.command",
        "Open Local Browser Bridge.command",
        "Start Computer Helper.command",
    ] {
        assert!(
            macos.contains(required),
            "macOS guided setup is missing `{required}`"
        );
    }
    assert!(windows_docs.contains("Start menu"));
    assert!(macos_docs.contains("three double-click launchers"));
}

#[test]
fn shell_startup_authority_is_explicit_and_reversible() {
    let windows = source("scripts/install-windows.ps1");
    let macos = source("scripts/install-macos.sh");
    assert!(windows.contains("[switch]$EnableShell"));
    assert!(windows.contains("if ($EnableShell) { \" --enable-shell\" } else { \"\" }"));
    assert!(windows.contains("Shell access is off"));
    assert!(macos.contains("enable_shell=0"));
    assert!(macos.contains("--enable-shell) enable_shell=1"));
    assert!(macos.contains("Shell access is off"));
}

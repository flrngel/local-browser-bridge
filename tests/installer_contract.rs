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
        "$startupArguments = if ($EnableShell)",
        "New-OwnedShortcut $startup $desktopPath $startupArguments",
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
        "((start_helper)) && desktop_launch_arguments+=(--start-helper)",
    ] {
        assert!(
            macos.contains(required),
            "macOS installer is missing `{required}`"
        );
    }
    assert!(!windows.contains("Set-MpPreference"));
    assert!(!windows.contains("ExecutionPolicy Bypass"));
    assert!(!windows.contains("New-OwnedShortcut $startup $desktopPath $launchArgumentText"));
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
fn uninstallers_are_one_command_owned_and_fail_closed() {
    let windows = source("scripts/uninstall-windows.ps1");
    let macos = source("scripts/uninstall-macos.sh");
    let readme = source("README.md");
    let install = source("docs/VERIFY_RELEASE.md");

    for required in [
        "[switch]$DryRun",
        "[switch]$KeepToken",
        "[switch]$NoBrowser",
        ".lbb-install-owner",
        "local-browser-bridge-install-v1",
        "Test-AllowlistedInstallName",
        "Refusing a product tree containing a reparse point",
        "Browser profiles were not edited",
        "Windows one-command uninstaller self-test passed.",
    ] {
        assert!(
            windows.contains(required),
            "Windows uninstaller is missing `{required}`"
        );
    }
    for required in [
        "--dry-run",
        "--keep-token",
        "--keep-permissions",
        "--no-browser",
        ".lbb-install-owner",
        "local-browser-bridge-install-v1",
        "is_allowlisted_top_level_name",
        "Refusing a product tree containing a symlink",
        "Browser profiles were not edited",
        "macOS one-command uninstaller self-test passed.",
    ] {
        assert!(
            macos.contains(required),
            "macOS uninstaller is missing `{required}`"
        );
    }
    assert!(readme.contains("## Uninstall in one command"));
    assert!(readme.contains("scripts/uninstall-windows.ps1"));
    assert!(readme.contains("scripts/uninstall-macos.sh | bash"));
    assert!(install.contains("There is no Linux package"));
    assert!(windows.contains("Browser profile files were intentionally left untouched"));
    assert!(macos.contains("Browser profile files were intentionally left untouched"));
    assert!(!windows.contains("Preferences"));
    assert!(!macos.contains("Library/Application Support/Google/Chrome"));
}

#[test]
fn installers_create_owned_version_matched_uninstall_launchers() {
    let windows = source("scripts/install-windows.ps1");
    let macos = source("scripts/install-macos.sh");
    for required in [
        ".lbb-install-owner",
        "local-browser-bridge-install-v1",
        "Uninstall Local Browser Bridge.lnk",
        "/v$resolved/scripts/uninstall-windows.ps1",
    ] {
        assert!(
            windows.contains(required),
            "Windows installer is missing `{required}`"
        );
    }
    for required in [
        ".lbb-install-owner",
        "local-browser-bridge-install-v1",
        "Uninstall Local Browser Bridge.command",
        "/v$resolved/scripts/uninstall-macos.sh",
    ] {
        assert!(
            macos.contains(required),
            "macOS installer is missing `{required}`"
        );
    }
}

#[test]
fn existing_ci_jobs_self_test_the_installers() {
    let ci = source(".github/workflows/ci.yml");
    assert!(ci.contains("bash scripts/install-macos.sh --self-test"));
    assert!(ci.contains("bash scripts/uninstall-macos.sh --self-test"));
    assert!(ci.contains("\"scripts/install-windows.ps1\""));
    assert!(ci.contains("\"scripts/uninstall-windows.ps1\""));
    assert!(ci.contains("-File ./scripts/install-windows.ps1 -SelfTest"));
    assert!(ci.contains("-File ./scripts/uninstall-windows.ps1 -SelfTest"));
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
        "Finish Browser Extension Setup.lnk",
        "Local Browser Bridge.lnk",
        "--extension-setup",
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
    assert!(macos_docs.contains("four maintenance launchers"));
}

#[test]
fn shell_startup_authority_is_explicit_and_reversible() {
    let windows = source("scripts/install-windows.ps1");
    let macos = source("scripts/install-macos.sh");
    assert!(windows.contains("[switch]$EnableShell"));
    assert!(windows.contains("$startupArguments = if ($EnableShell)"));
    assert!(windows.contains("if ($EnableShell) { $launchArguments += \"--enable-shell\" }"));
    assert!(windows.contains("Shell access is off"));
    assert!(macos.contains("enable_shell=0"));
    assert!(macos.contains("--enable-shell) enable_shell=1"));
    assert!(macos.contains("Shell access is off"));
}

#[test]
fn normal_startup_uses_the_desktop_host_without_a_console_window() {
    let desktop = source("src/bin/local-browser-bridge-desktop.rs");
    let windows = source("scripts/install-windows.ps1");
    let macos = source("scripts/install-macos.sh");
    let desktop_plist = source("packaging/macos/DesktopInfo.plist.in");

    assert!(desktop.contains("windows_subsystem = \"windows\""));
    assert!(desktop.contains("CREATE_NO_WINDOW"));
    assert!(windows.contains("$desktopPath = Join-Path $InstallRoot $serverName"));
    assert!(windows.contains("New-OwnedShortcut $startup $desktopPath"));
    assert!(!windows.contains("Start-Process -FilePath $serverPath -WindowStyle Minimized"));
    assert!(macos.contains("Local Browser Bridge.app/Contents/MacOS/local-browser-bridge-desktop"));
    assert!(macos.contains("<key>SuccessfulExit</key><false/>"));
    assert!(desktop_plist.contains("<key>LSUIElement</key>"));
    assert!(desktop_plist.contains("<true/>"));
    let helper = source("src/bin/local-computer-helper.rs");
    assert!(helper.contains("CREATE_SUSPENDED | CREATE_NO_WINDOW"));
    assert!(helper.contains("controllerProcessId"));
}

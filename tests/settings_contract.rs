//! Contract tests for `src/settings.rs`: the settings file must never fail
//! startup (missing, unreadable, or corrupt all fall back to defaults), a
//! round-trip must be byte-faithful, unknown keys must be ignored rather than
//! rejected, and `LBB_SETTINGS_PATH` must be honored.

use std::path::Path;

use local_browser_bridge::{Settings, default_settings_path, load_settings, save_settings};
use tempfile::tempdir;

#[tokio::test]
async fn defaults_when_settings_file_is_absent() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");

    let settings = load_settings(&path).await;

    assert_eq!(settings, Settings::default());
    assert!(settings.shell_enabled);
    assert!(settings.desktop_control_enabled);
    assert!(settings.start_at_login);
}

#[tokio::test]
async fn defaults_when_settings_file_is_corrupt_json() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    tokio::fs::write(&path, b"{ this is not json")
        .await
        .unwrap();

    assert_eq!(load_settings(&path).await, Settings::default());
}

#[tokio::test]
async fn defaults_when_settings_file_has_an_unrecognized_version() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    tokio::fs::write(
        &path,
        br#"{"version":2,"shellEnabled":false,"desktopControlEnabled":false,"startAtLogin":false}"#,
    )
    .await
    .unwrap();

    // A future or otherwise-unrecognized version is treated exactly like a
    // corrupt file: never the individual values it happens to carry.
    assert_eq!(load_settings(&path).await, Settings::default());
}

#[tokio::test]
async fn defaults_when_settings_file_has_no_version_field() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    tokio::fs::write(&path, br#"{"shellEnabled":false}"#)
        .await
        .unwrap();

    assert_eq!(load_settings(&path).await, Settings::default());
}

#[tokio::test]
async fn round_trips_a_saved_settings_file_atomically_and_privately() {
    let dir = tempdir().unwrap();
    // Nested to also prove save_settings creates its parent directory.
    let path = dir.path().join("nested").join("settings.json");
    let saved = Settings {
        shell_enabled: false,
        desktop_control_enabled: true,
        start_at_login: false,
    };

    save_settings(&path, &saved).await.unwrap();

    assert_eq!(load_settings(&path).await, saved);

    // No stray temporary file was left behind next to the final file.
    let mut names: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect();
    assert_eq!(names.len(), 1);
    assert_eq!(names.pop().unwrap(), "settings.json");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "settings.json must be created private (0600)");
    }
}

#[tokio::test]
async fn saving_twice_replaces_the_file_instead_of_failing() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");

    save_settings(&path, &Settings::default()).await.unwrap();
    let second = Settings {
        shell_enabled: false,
        desktop_control_enabled: false,
        start_at_login: false,
    };
    save_settings(&path, &second).await.unwrap();

    assert_eq!(load_settings(&path).await, second);
}

#[tokio::test]
async fn unknown_extra_json_keys_are_ignored_not_rejected() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("settings.json");
    tokio::fs::write(
        &path,
        br#"{
            "version": 1,
            "shellEnabled": false,
            "desktopControlEnabled": true,
            "startAtLogin": false,
            "somethingThisBuildDoesNotKnowAbout": "unused",
            "nested": { "a": 1 }
        }"#,
    )
    .await
    .unwrap();

    let settings = load_settings(&path).await;

    assert_eq!(
        settings,
        Settings {
            shell_enabled: false,
            desktop_control_enabled: true,
            start_at_login: false,
        }
    );
}

/// `LBB_SETTINGS_PATH` is a process-global override, so this is the only
/// test in the crate allowed to touch it; it always restores whatever value
/// (or absence) it found.
#[test]
fn default_settings_path_honours_the_env_override() {
    let previous = std::env::var_os("LBB_SETTINGS_PATH");

    unsafe {
        std::env::set_var(
            "LBB_SETTINGS_PATH",
            "/tmp/lbb-settings-contract-override.json",
        );
    }
    let path = default_settings_path().expect("an explicit override always resolves");
    assert_eq!(path, Path::new("/tmp/lbb-settings-contract-override.json"));

    unsafe {
        match &previous {
            Some(value) => std::env::set_var("LBB_SETTINGS_PATH", value),
            None => std::env::remove_var("LBB_SETTINGS_PATH"),
        }
    }
}

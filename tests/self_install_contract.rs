//! Contract tests for `src/setup.rs`: the platform-independent parts of the
//! Windows self-installer — the embedded extension writer, the pure path
//! logic behind `is_installed()`, and the helper-name fallback order — plus
//! Windows-only assertions about the install location and executable name.
//!
//! The impure Windows install/uninstall/registry/shortcut machinery is not
//! exercised here: it needs a real Windows machine, and its pure decision
//! logic (which name to look for, whether a path counts as "installed") is
//! exactly what has been split out into testable functions below.

use std::fs;

use local_browser_bridge::setup;
use tempfile::tempdir;

/// The exact file set `extension/` ships today. Kept as a literal list (like
/// `tests/extension_contract.rs`'s own `EXTENSION_FILES` does for the source
/// tree) so a file silently missing from the embedded copy is caught here.
const EXPECTED_EXTENSION_FILES: &[&str] = &[
    "background.js",
    "content.js",
    "dom-core.js",
    "frame-agent.js",
    "lib.js",
    "manifest.json",
    "pair.html",
    "pair.js",
    "popup.css",
    "popup.html",
    "popup.js",
];

#[test]
fn embedded_extension_writes_the_full_file_set() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("extension");

    setup::write_embedded_extension(&target).unwrap();

    let mut written: Vec<String> = fs::read_dir(&target)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    written.sort();
    let mut expected: Vec<String> = EXPECTED_EXTENSION_FILES
        .iter()
        .map(|name| (*name).to_owned())
        .collect();
    expected.sort();
    assert_eq!(written, expected);
}

#[test]
fn embedded_extension_manifest_is_valid_and_matches_the_source_tree() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("extension");

    setup::write_embedded_extension(&target).unwrap();

    let written = fs::read(target.join("manifest.json")).unwrap();
    let on_disk = fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("extension")
            .join("manifest.json"),
    )
    .unwrap();
    assert_eq!(
        written, on_disk,
        "embedded manifest.json must match the source tree exactly"
    );

    let manifest: serde_json::Value = serde_json::from_slice(&written).unwrap();
    assert_eq!(manifest["manifest_version"], 3);
    assert!(
        manifest["key"].is_string(),
        "the pinned extension key must survive embedding"
    );
}

#[test]
fn embedded_extension_write_is_idempotent_and_overwrites_stale_content() {
    let dir = tempdir().unwrap();
    let target = dir.path().join("extension");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("manifest.json"), b"stale").unwrap();

    setup::write_embedded_extension(&target).unwrap();
    // Writing again (e.g. a repair run) must not fail or duplicate anything.
    setup::write_embedded_extension(&target).unwrap();

    let manifest = fs::read_to_string(target.join("manifest.json")).unwrap();
    assert_ne!(
        manifest, "stale",
        "an existing manifest.json must be overwritten with the embedded copy"
    );
}

#[cfg(unix)]
#[test]
fn embedded_extension_never_follows_a_symlink_out_of_the_target() {
    use std::os::unix::fs::symlink;

    let dir = tempdir().unwrap();
    let target = dir.path().join("extension");
    let outside = dir.path().join("outside");
    fs::create_dir_all(&outside).unwrap();
    fs::create_dir_all(&target).unwrap();
    // A pre-existing symlink at the exact path a normal file would land on.
    symlink(&outside, target.join("manifest.json")).unwrap();

    setup::write_embedded_extension(&target).unwrap();

    let metadata = fs::symlink_metadata(target.join("manifest.json")).unwrap();
    assert!(
        !metadata.file_type().is_symlink(),
        "the symlink must be replaced, not written through"
    );
    assert!(target.join("manifest.json").is_file());
    assert!(
        fs::read_dir(&outside).unwrap().next().is_none(),
        "nothing should ever have been written into the symlink's target directory"
    );
}

#[test]
fn helper_candidate_names_prefers_the_stable_name_then_the_legacy_versioned_name() {
    let names = setup::helper_candidate_names("1.2.3");
    assert_eq!(
        names,
        [
            "Local Computer Helper.exe".to_owned(),
            "local-computer-helper-v1.2.3-windows-x86_64.exe".to_owned(),
        ]
    );
}

#[test]
fn exe_parent_matches_root_true_for_a_file_directly_under_root() {
    let dir = tempdir().unwrap();
    let exe = dir.path().join("Local Browser Bridge.exe");
    fs::write(&exe, b"").unwrap();

    assert!(setup::exe_parent_matches_root(&exe, dir.path()));
}

#[test]
fn exe_parent_matches_root_false_for_an_unrelated_directory() {
    let a = tempdir().unwrap();
    let b = tempdir().unwrap();
    let exe = a.path().join("Local Browser Bridge.exe");

    assert!(!setup::exe_parent_matches_root(&exe, b.path()));
}

#[test]
fn exe_parent_matches_root_false_when_the_candidate_has_no_parent() {
    assert!(!setup::exe_parent_matches_root(
        std::path::Path::new(""),
        std::path::Path::new("/anywhere")
    ));
}

#[test]
fn exe_parent_matches_root_is_case_insensitive_for_paths_that_do_not_exist() {
    // Neither side exists, so this exercises the case-insensitive string
    // fallback (real installs are always on a case-insensitive Windows
    // volume, unlike this test host).
    let exe = std::path::Path::new("/does/not/exist/Programs/Local Browser Bridge/App.exe");
    let root = std::path::Path::new("/DOES/NOT/EXIST/Programs/Local Browser Bridge");
    assert!(setup::exe_parent_matches_root(exe, root));
}

#[cfg(target_os = "windows")]
#[test]
fn installed_exe_name_is_stable_and_human_readable() {
    assert_eq!(setup::INSTALLED_EXE_NAME, "Local Browser Bridge.exe");
}

#[cfg(target_os = "windows")]
#[test]
fn install_root_is_local_appdata_programs_local_browser_bridge() {
    let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") else {
        return;
    };
    let expected = std::path::PathBuf::from(local_app_data)
        .join("Programs")
        .join("Local Browser Bridge");
    assert_eq!(setup::install_root().unwrap(), expected);
}

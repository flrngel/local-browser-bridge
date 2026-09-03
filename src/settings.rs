//! User-facing settings persisted at `<home>/.local-browser-bridge/settings.json`
//! (override with `LBB_SETTINGS_PATH`), so shell and desktop-control access
//! survive a restart without re-running the installer.
//!
//! Loading is infallible by design: a missing file, an unreadable file, bad
//! JSON, or an unrecognized `version` all fall back to the all-`true`
//! defaults rather than failing startup. Saving is atomic (write a temp file,
//! then rename) so a crash or concurrent read never observes a half-written
//! file, and the file is created private (mode 0600) on Unix.

use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The only settings-file schema version this build understands. An absent
/// or different `version` is treated exactly like a corrupt file.
const SETTINGS_VERSION: u64 = 1;
const SETTINGS_DIRECTORY: &str = ".local-browser-bridge";
const SETTINGS_FILE_NAME: &str = "settings.json";

/// The current user's shell/desktop-control/login preferences. Every field
/// defaults to `true`: the installer's whole point is that desktop control
/// and shell access work without a second setup step.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Settings {
    pub shell_enabled: bool,
    pub desktop_control_enabled: bool,
    pub start_at_login: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            shell_enabled: true,
            desktop_control_enabled: true,
            start_at_login: true,
        }
    }
}

/// The on-disk JSON shape. Every field is optional so a partially written or
/// hand-edited file never fails to parse; a value missing from an otherwise
/// valid file falls back to that field's individual default. Unknown extra
/// keys are ignored (serde's default behavior: no `deny_unknown_fields`).
#[derive(Serialize, Deserialize)]
struct SettingsFile {
    #[serde(default)]
    version: Option<u64>,
    #[serde(default, rename = "shellEnabled")]
    shell_enabled: Option<bool>,
    #[serde(default, rename = "desktopControlEnabled")]
    desktop_control_enabled: Option<bool>,
    #[serde(default, rename = "startAtLogin")]
    start_at_login: Option<bool>,
}

/// The default settings file path, honoring `LBB_SETTINGS_PATH` when set.
pub fn default_settings_path() -> io::Result<PathBuf> {
    if let Some(path) = std::env::var_os("LBB_SETTINGS_PATH") {
        if path.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "LBB_SETTINGS_PATH is set but empty",
            ));
        }
        return Ok(PathBuf::from(path));
    }
    let Some(home) = crate::home_dir().filter(|path| path.is_absolute()) else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "could not resolve an absolute current-user profile directory; set LBB_SETTINGS_PATH to a settings file path",
        ));
    };
    Ok(home.join(SETTINGS_DIRECTORY).join(SETTINGS_FILE_NAME))
}

/// Loads settings from `path`. Never fails: any problem reading or parsing
/// the file (missing, unreadable, corrupt JSON, unknown `version`) yields
/// `Settings::default()` instead of an error, so a broken settings file can
/// never block startup.
pub async fn load_settings(path: &Path) -> Settings {
    let Ok(contents) = tokio::fs::read(path).await else {
        return Settings::default();
    };
    let Ok(file) = serde_json::from_slice::<SettingsFile>(&contents) else {
        return Settings::default();
    };
    if file.version != Some(SETTINGS_VERSION) {
        return Settings::default();
    }
    let defaults = Settings::default();
    Settings {
        shell_enabled: file.shell_enabled.unwrap_or(defaults.shell_enabled),
        desktop_control_enabled: file
            .desktop_control_enabled
            .unwrap_or(defaults.desktop_control_enabled),
        start_at_login: file.start_at_login.unwrap_or(defaults.start_at_login),
    }
}

/// Writes `settings` to `path` atomically: a sibling temporary file is
/// created private (mode 0600 on Unix), written, flushed, and renamed over
/// `path`. A rename is a single filesystem operation, so a concurrent reader
/// (or a crash mid-write) never observes a partially written file.
pub async fn save_settings(path: &Path, settings: &Settings) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    tokio::fs::create_dir_all(parent).await?;

    let file = SettingsFile {
        version: Some(SETTINGS_VERSION),
        shell_enabled: Some(settings.shell_enabled),
        desktop_control_enabled: Some(settings.desktop_control_enabled),
        start_at_login: Some(settings.start_at_login),
    };
    let contents = serde_json::to_vec_pretty(&file)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;

    let temp_path = parent.join(format!(".settings-{}.tmp", uuid::Uuid::new_v4().simple()));
    let write_result = write_new_private_file(&temp_path, &contents).await;
    if write_result.is_err() {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return write_result;
    }
    if let Err(error) = tokio::fs::rename(&temp_path, path).await {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(error);
    }
    Ok(())
}

#[cfg(unix)]
async fn write_new_private_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    use tokio::io::AsyncWriteExt as _;

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .await?;
    file.write_all(contents).await?;
    file.flush().await?;
    file.sync_all().await
}

#[cfg(not(unix))]
async fn write_new_private_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    use tokio::io::AsyncWriteExt as _;

    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .await?;
    file.write_all(contents).await?;
    file.flush().await?;
    file.sync_all().await
}

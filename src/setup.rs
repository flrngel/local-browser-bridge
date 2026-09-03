//! Turns the standalone desktop executable into a self-installing app.
//!
//! `write_embedded_extension` is cross-platform: the browser extension
//! directory is embedded into the binary at compile time (the same
//! `include_dir!` trick `src/server.rs` already uses for `public/`), so a
//! single downloaded executable can always lay out a working unpacked
//! extension without shipping a second file or folder alongside it.
//!
//! Everything else here is Windows-only. Windows ships no signed installer
//! for this project, so the desktop executable installs itself: it copies
//! itself into a stable per-user location, registers a Start Menu shortcut
//! and a start-at-login entry, and can undo all of that again. macOS instead
//! ships a signed `.app` bundle from a DMG/installer script, so there is
//! nothing analogous to self-install to do there.

use std::fs;
use std::io;
use std::path::Path;
#[cfg(target_os = "windows")]
use std::path::PathBuf;

use include_dir::{Dir, DirEntry, include_dir};

/// The `extension/` directory, embedded into the binary at compile time.
const EXTENSION_DIR: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/extension");

/// Writes the embedded extension tree to `target`: creates directories as
/// needed and overwrites existing files. Any symlink found at a path this
/// function is about to write through is removed first, so a write can
/// never be redirected outside `target` by a pre-existing symlink.
pub fn write_embedded_extension(target: &Path) -> io::Result<()> {
    write_dir_entries(&EXTENSION_DIR, target)
}

fn write_dir_entries(dir: &Dir<'_>, target: &Path) -> io::Result<()> {
    replace_with_real_dir(target)?;
    for entry in dir.entries() {
        match entry {
            DirEntry::Dir(sub) => {
                let name = entry_file_name(sub.path())?;
                write_dir_entries(sub, &target.join(name))?;
            }
            DirEntry::File(file) => {
                let name = entry_file_name(file.path())?;
                write_file(&target.join(name), file.contents())?;
            }
        }
    }
    Ok(())
}

fn entry_file_name(path: &Path) -> io::Result<&std::ffi::OsStr> {
    path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "embedded entry has no file name",
        )
    })
}

/// Ensures `path` is a real (non-symlink) directory, replacing whatever is
/// already there — including a symlink or a plain file — first.
fn replace_with_real_dir(path: &Path) -> io::Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && (metadata.file_type().is_symlink() || !metadata.is_dir())
    {
        remove_existing(path, &metadata)?;
    }
    fs::create_dir_all(path)
}

fn write_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(path)
        && metadata.file_type().is_symlink()
    {
        remove_existing(path, &metadata)?;
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

fn remove_existing(path: &Path, metadata: &fs::Metadata) -> io::Result<()> {
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
    } else {
        fs::remove_file(path)
    }
}

/// Installed helper file names, in the order `helper_path()` should try
/// them: the new stable name first, then the legacy versioned name so an
/// existing install (from before the stable name existed) keeps working.
/// Pure and platform-independent so the naming order itself is testable
/// without a real installed layout.
pub fn helper_candidate_names(version: &str) -> [String; 2] {
    [
        "Local Computer Helper.exe".to_owned(),
        format!("local-computer-helper-v{version}-windows-x86_64.exe"),
    ]
}

/// Returns `true` when `candidate`'s parent directory refers to the same
/// filesystem location as `root`. This is the pure logic behind
/// `is_installed()` (which compares the running executable's own path
/// against `install_root()`), split out so it can be unit-tested without a
/// real Windows install layout.
pub fn exe_parent_matches_root(candidate: &Path, root: &Path) -> bool {
    match candidate.parent() {
        Some(parent) => paths_match(parent, root),
        None => false,
    }
}

/// Compares two paths as "the same location": canonicalized when both exist
/// (so short names, `..`, and symlinks resolve to the same answer), falling
/// back to a case-insensitive string compare (Windows paths are
/// case-insensitive) when either side does not exist yet.
fn paths_match(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a
            .to_string_lossy()
            .eq_ignore_ascii_case(&b.to_string_lossy()),
    }
}

#[cfg(target_os = "windows")]
mod windows_install {
    use std::os::windows::ffi::OsStrExt as _;
    use std::time::Duration;

    use super::{Path, PathBuf, fs, io, write_embedded_extension};

    /// The stable installed executable name, chosen so it survives upgrades
    /// (unlike the versioned release-asset file name).
    pub const INSTALLED_EXE_NAME: &str = "Local Browser Bridge.exe";
    const START_MENU_SHORTCUT_NAME: &str = "Local Browser Bridge.lnk";
    const RUN_VALUE_NAME: &str = "LocalBrowserBridge";
    const RUN_SUBKEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
    const KEY_SET_VALUE: u32 = 0x0002;
    const REG_SZ: u32 = 1;
    const ERROR_FILE_NOT_FOUND: i32 = 2;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn RegCreateKeyExW(
            hkey: *mut std::ffi::c_void,
            lp_sub_key: *const u16,
            reserved: u32,
            lp_class: *mut u16,
            dw_options: u32,
            sam_desired: u32,
            lp_security_attributes: *const std::ffi::c_void,
            phk_result: *mut *mut std::ffi::c_void,
            lpdw_disposition: *mut u32,
        ) -> i32;
        fn RegOpenKeyExW(
            hkey: *mut std::ffi::c_void,
            lp_sub_key: *const u16,
            ul_options: u32,
            sam_desired: u32,
            phk_result: *mut *mut std::ffi::c_void,
        ) -> i32;
        fn RegSetValueExW(
            hkey: *mut std::ffi::c_void,
            lp_value_name: *const u16,
            reserved: u32,
            dw_type: u32,
            lp_data: *const u8,
            cb_data: u32,
        ) -> i32;
        fn RegDeleteValueW(hkey: *mut std::ffi::c_void, lp_value_name: *const u16) -> i32;
        fn RegCloseKey(hkey: *mut std::ffi::c_void) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
    }

    fn hkey_current_user() -> *mut std::ffi::c_void {
        0x8000_0001_usize as *mut std::ffi::c_void
    }

    /// `%LOCALAPPDATA%\Programs\Local Browser Bridge`: the one stable
    /// location this app ever installs itself into.
    pub fn install_root() -> io::Result<PathBuf> {
        let local_app_data = std::env::var_os("LOCALAPPDATA")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "LOCALAPPDATA is not set"))?;
        Ok(PathBuf::from(local_app_data)
            .join("Programs")
            .join("Local Browser Bridge"))
    }

    /// `true` when the currently running executable already lives in
    /// `install_root()`.
    pub fn is_installed() -> io::Result<bool> {
        let root = install_root()?;
        let current_exe = std::env::current_exe()?;
        Ok(super::exe_parent_matches_root(&current_exe, &root))
    }

    /// Installs the running executable into `install_root()` alongside the
    /// embedded extension and default settings, then registers a Start Menu
    /// shortcut and a start-at-login entry. Returns the installed
    /// executable's path. A failed shortcut or login-entry step is reported
    /// through `log` but never fails the install itself; only a failure to
    /// copy the executable, write the extension, or create the install
    /// folder is fatal.
    pub fn install(log: &mut dyn FnMut(&str)) -> io::Result<PathBuf> {
        let root = install_root()?;
        fs::create_dir_all(&root)?;
        let current_exe = std::env::current_exe()?;
        let dest_exe = root.join(INSTALLED_EXE_NAME);
        copy_running_executable(&current_exe, &dest_exe)?;
        write_embedded_extension(&root.join("extension"))?;
        write_default_settings_if_absent(log);
        if let Err(error) = create_start_menu_shortcut(&dest_exe) {
            log(&format!(
                "Could not create the Start Menu shortcut: {error}"
            ));
        }
        if let Err(error) = set_start_at_login(&dest_exe) {
            log(&format!("Could not set up start-at-login: {error}"));
        }
        Ok(dest_exe)
    }

    /// Removes the Start Menu shortcut, the start-at-login entry, and the
    /// install folder (including the running executable's own file — modern
    /// Windows shares delete access on an executing image, so this succeeds
    /// even while this process is that executable). Shortcut/login-entry
    /// removal failures are logged and do not stop the folder removal.
    pub fn uninstall(log: &mut dyn FnMut(&str)) -> io::Result<()> {
        if let Err(error) = remove_start_at_login() {
            log(&format!(
                "Could not remove the start-at-login entry: {error}"
            ));
        }
        if let Err(error) = remove_start_menu_shortcut() {
            log(&format!(
                "Could not remove the Start Menu shortcut: {error}"
            ));
        }
        let root = install_root()?;
        if root.is_dir() {
            fs::remove_dir_all(&root)?;
        }
        Ok(())
    }

    fn write_default_settings_if_absent(log: &mut dyn FnMut(&str)) {
        let path = match crate::settings::default_settings_path() {
            Ok(path) => path,
            Err(error) => {
                log(&format!(
                    "Could not resolve the settings file path: {error}"
                ));
                return;
            }
        };
        if path.exists() {
            return;
        }
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                log(&format!("Could not write default settings: {error}"));
                return;
            }
        };
        let result = runtime.block_on(crate::settings::save_settings(
            &path,
            &crate::settings::Settings::default(),
        ));
        if let Err(error) = result {
            log(&format!("Could not write default settings: {error}"));
        }
    }

    /// Copies `source` over `dest`. When `dest` does not exist yet this is a
    /// plain copy. When it does (an upgrade, or a self-install run from the
    /// already-installed copy), Windows may hold the existing file open for
    /// execution, which blocks writing to it in place — so the new bytes are
    /// written to a temp file next to `dest` first and swapped in with
    /// `MoveFileExW(MOVEFILE_REPLACE_EXISTING)`, which can replace a file
    /// that is merely open (as opposed to exclusively locked). If that still
    /// fails, the app now running from `dest` is genuinely blocking the
    /// write, so a clear, actionable error is returned instead.
    fn copy_running_executable(source: &Path, dest: &Path) -> io::Result<()> {
        if !dest.exists() {
            fs::copy(source, dest)?;
            return Ok(());
        }
        let file_name = dest
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(INSTALLED_EXE_NAME);
        let temp = dest.with_file_name(format!(".{file_name}.new"));
        let _ = fs::remove_file(&temp);
        fs::copy(source, &temp)?;
        let result = move_file_replace(&temp, dest);
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result.map_err(|error| {
            io::Error::new(
                error.kind(),
                format!(
                    "Local Browser Bridge is already running from {}. Quit it from its tray icon first, then try again. ({error})",
                    dest.display()
                ),
            )
        })
    }

    fn move_file_replace(from: &Path, to: &Path) -> io::Result<()> {
        let from_wide = wide_null_path(from);
        let to_wide = wide_null_path(to);
        let ok = unsafe {
            MoveFileExW(
                from_wide.as_ptr(),
                to_wide.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if ok != 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    fn set_start_at_login(target_exe: &Path) -> io::Result<()> {
        let subkey = wide_null(RUN_SUBKEY);
        let mut key: *mut std::ffi::c_void = std::ptr::null_mut();
        let status = unsafe {
            RegCreateKeyExW(
                hkey_current_user(),
                subkey.as_ptr(),
                0,
                std::ptr::null_mut(),
                0,
                KEY_SET_VALUE,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            )
        };
        if status != 0 {
            return Err(io::Error::from_raw_os_error(status));
        }
        let value_name = wide_null(RUN_VALUE_NAME);
        let command = format!("\"{}\"", target_exe.display());
        let data = wide_null(&command);
        let data_bytes: &[u8] =
            unsafe { std::slice::from_raw_parts(data.as_ptr().cast::<u8>(), data.len() * 2) };
        let set_status = unsafe {
            RegSetValueExW(
                key,
                value_name.as_ptr(),
                0,
                REG_SZ,
                data_bytes.as_ptr(),
                data_bytes.len() as u32,
            )
        };
        unsafe { RegCloseKey(key) };
        if set_status != 0 {
            return Err(io::Error::from_raw_os_error(set_status));
        }
        Ok(())
    }

    fn remove_start_at_login() -> io::Result<()> {
        let subkey = wide_null(RUN_SUBKEY);
        let mut key: *mut std::ffi::c_void = std::ptr::null_mut();
        let status = unsafe {
            RegOpenKeyExW(
                hkey_current_user(),
                subkey.as_ptr(),
                0,
                KEY_SET_VALUE,
                &mut key,
            )
        };
        if status != 0 {
            return if status == ERROR_FILE_NOT_FOUND {
                Ok(())
            } else {
                Err(io::Error::from_raw_os_error(status))
            };
        }
        let value_name = wide_null(RUN_VALUE_NAME);
        let delete_status = unsafe { RegDeleteValueW(key, value_name.as_ptr()) };
        unsafe { RegCloseKey(key) };
        if delete_status != 0 && delete_status != ERROR_FILE_NOT_FOUND {
            return Err(io::Error::from_raw_os_error(delete_status));
        }
        Ok(())
    }

    fn start_menu_programs_dir() -> io::Result<PathBuf> {
        let appdata = std::env::var_os("APPDATA")
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "APPDATA is not set"))?;
        Ok(PathBuf::from(appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs"))
    }

    fn create_start_menu_shortcut(target_exe: &Path) -> io::Result<()> {
        let programs = start_menu_programs_dir()?;
        fs::create_dir_all(&programs)?;
        let working_dir = target_exe
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| programs.clone());
        let link_path = programs.join(START_MENU_SHORTCUT_NAME);
        create_shortcut_via_powershell(&link_path, target_exe, &working_dir, "Local Browser Bridge")
    }

    /// Creates a `.lnk` shortcut by shelling out to `powershell.exe` and
    /// asking `WScript.Shell` (always present on Windows) to build and save
    /// it, rather than hand-writing the MS-SHLLINK binary format ourselves —
    /// that format was never validated against real Explorer, while this is
    /// exactly the approach `scripts/install-windows.ps1` already uses
    /// successfully.
    fn create_shortcut_via_powershell(
        link_path: &Path,
        target_exe: &Path,
        working_dir: &Path,
        description: &str,
    ) -> io::Result<()> {
        let command = format!(
            "$s = New-Object -ComObject WScript.Shell; \
             $l = $s.CreateShortcut('{link}'); \
             $l.TargetPath = '{target}'; \
             $l.WorkingDirectory = '{workdir}'; \
             $l.Description = '{description}'; \
             $l.Save()",
            link = powershell_single_quote(link_path),
            target = powershell_single_quote(target_exe),
            workdir = powershell_single_quote(working_dir),
            description = powershell_single_quote_str(description),
        );
        let output = std::process::Command::new("powershell")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &command,
            ])
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(io::Error::other(format!(
                "powershell exited with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }

    /// Escapes `value` for embedding inside a PowerShell single-quoted
    /// string literal: a literal single quote is escaped by doubling it.
    fn powershell_single_quote_str(value: &str) -> String {
        value.replace('\'', "''")
    }

    fn powershell_single_quote(path: &Path) -> String {
        powershell_single_quote_str(&path.display().to_string())
    }

    fn remove_start_menu_shortcut() -> io::Result<()> {
        let path = start_menu_programs_dir()?.join(START_MENU_SHORTCUT_NAME);
        match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn wide_null(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn wide_null_path(path: &Path) -> Vec<u16> {
        path.as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    #[derive(serde::Deserialize)]
    struct ReleaseMeta {
        assets: Vec<AssetMeta>,
    }

    #[derive(serde::Deserialize)]
    struct AssetMeta {
        name: String,
        browser_download_url: String,
        digest: Option<String>,
    }

    fn sha256_hex(bytes: &[u8]) -> String {
        use sha2::{Digest as _, Sha256};
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect()
    }

    /// Hard ceiling on the Computer Helper download, enforced while
    /// streaming (never trusted from a server-controlled `Content-Length`
    /// alone). 200 MB is generous headroom for this binary.
    const MAX_HELPER_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024;

    /// The metadata request (GitHub release lookup) is small and should
    /// fail fast; the binary download itself needs a much larger budget
    /// because in `reqwest` a client-level `.timeout()` bounds the *entire*
    /// request including the whole response body, so a 30-second budget
    /// would abort a legitimate download on a slow connection. Each request
    /// gets its own explicit override rather than relying on one shared
    /// client default.
    const METADATA_TIMEOUT: Duration = Duration::from_secs(30);
    const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);

    /// Serializes `download_helper` calls so rapidly re-toggling desktop
    /// control cannot start duplicate concurrent downloads racing to
    /// install the same file: a second caller waits for the first to finish
    /// instead of starting its own transfer.
    static DOWNLOAD_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

    /// Ensures a working Computer Helper executable exists under `root`,
    /// downloading and verifying it from the GitHub release matching this
    /// build's version if it is missing, and reports progress throughout via
    /// `monitor.set_helper_setup` so the web dashboard can show one honest
    /// sentence instead of a developer console. Returns the helper path on
    /// success; on failure the monitor already carries the "failed" state
    /// and a plain-language, actionable message.
    pub async fn ensure_helper_downloaded(
        root: &Path,
        monitor: &crate::BridgeStatusMonitor,
    ) -> Option<PathBuf> {
        if let Some(existing) = super::helper_candidate_names(crate::VERSION)
            .into_iter()
            .map(|name| root.join(name))
            .find(|candidate| candidate.is_file())
        {
            monitor
                .set_helper_setup("ready", 100, "The Computer Helper is ready.")
                .await;
            return Some(existing);
        }
        monitor
            .set_helper_setup("downloading", 0, "Looking up the Computer Helper release…")
            .await;
        match download_helper(root, monitor).await {
            Ok(path) => {
                monitor
                    .set_helper_setup("ready", 100, "The Computer Helper is ready.")
                    .await;
                Some(path)
            }
            Err(message) => {
                monitor.set_helper_setup("failed", 0, &message).await;
                None
            }
        }
    }

    async fn download_helper(
        root: &Path,
        monitor: &crate::BridgeStatusMonitor,
    ) -> Result<PathBuf, String> {
        // Only one real download runs at a time. A second concurrent call
        // (e.g. a tray "retry" click racing the automatic flow) waits here,
        // then — once it has the lock — rechecks whether the first caller
        // already finished, short-circuiting instead of downloading again.
        let _guard = DOWNLOAD_LOCK.lock().await;
        if let Some(existing) = super::helper_candidate_names(crate::VERSION)
            .into_iter()
            .map(|name| root.join(name))
            .find(|candidate| candidate.is_file())
        {
            return Ok(existing);
        }

        let version = crate::VERSION;
        let tag = format!("v{version}");
        let api_url = format!(
            "https://api.github.com/repos/flrngel/local-browser-bridge/releases/tags/{tag}"
        );
        let asset_name = format!("local-computer-helper-v{version}-windows-x86_64.exe");

        let client = reqwest::Client::builder()
            .user_agent(format!("local-browser-bridge/{version} helper-setup"))
            .build()
            .map_err(|_| "Could not start the download client.".to_owned())?;

        let response = client
            .get(&api_url)
            .header("Accept", "application/vnd.github+json")
            .header("X-GitHub-Api-Version", "2026-03-10")
            .timeout(METADATA_TIMEOUT)
            .send()
            .await
            .map_err(|_| {
                "Could not reach GitHub to look up the Computer Helper release.".to_owned()
            })?;
        if !response.status().is_success() {
            return Err(format!(
                "GitHub returned HTTP {} while looking up the release {tag}.",
                response.status().as_u16()
            ));
        }
        let release: ReleaseMeta = response
            .json()
            .await
            .map_err(|_| "GitHub returned unexpected release metadata.".to_owned())?;
        let asset = release
            .assets
            .into_iter()
            .find(|asset| asset.name == asset_name)
            .ok_or_else(|| format!("The release {tag} does not have a {asset_name} asset."))?;
        let expected_digest = asset
            .digest
            .as_deref()
            .and_then(|digest| digest.strip_prefix("sha256:"))
            .filter(|hex| hex.len() == 64 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .map(str::to_owned)
            .ok_or_else(|| {
                "GitHub did not report a verifiable checksum for the Computer Helper.".to_owned()
            })?;

        monitor
            .set_helper_setup("downloading", 10, "Downloading the Computer Helper…")
            .await;
        let mut download = client
            .get(&asset.browser_download_url)
            .timeout(DOWNLOAD_TIMEOUT)
            .send()
            .await
            .map_err(|_| "Could not download the Computer Helper.".to_owned())?;
        if !download.status().is_success() {
            return Err(format!(
                "GitHub returned HTTP {} while downloading the Computer Helper.",
                download.status().as_u16()
            ));
        }
        // `Content-Length` is only a hint and is server-controlled, so it is
        // used solely to size the initial buffer and drive the progress
        // percentage — never trusted on its own. The declared size is still
        // rejected up front when it already exceeds the cap, and the actual
        // byte count streamed in is what enforces the ceiling below.
        let declared_total = download.content_length().filter(|&total| total > 0);
        if let Some(declared_total) = declared_total
            && declared_total > MAX_HELPER_DOWNLOAD_BYTES
        {
            return Err(format!(
                "The Computer Helper download reports a size of {declared_total} bytes, which exceeds the {MAX_HELPER_DOWNLOAD_BYTES}-byte limit; it was refused before downloading."
            ));
        }
        let mut bytes =
            Vec::with_capacity(declared_total.unwrap_or(0).min(MAX_HELPER_DOWNLOAD_BYTES) as usize);
        while let Some(chunk) = download
            .chunk()
            .await
            .map_err(|_| "The Computer Helper download was interrupted.".to_owned())?
        {
            if bytes.len() as u64 + chunk.len() as u64 > MAX_HELPER_DOWNLOAD_BYTES {
                return Err(format!(
                    "The Computer Helper download exceeded the {MAX_HELPER_DOWNLOAD_BYTES}-byte limit and was aborted."
                ));
            }
            bytes.extend_from_slice(&chunk);
            if let Some(declared_total) = declared_total {
                let percent = 10 + ((bytes.len() as u64 * 90) / declared_total).min(90) as u8;
                monitor
                    .set_helper_setup("downloading", percent, "Downloading the Computer Helper…")
                    .await;
            }
        }

        if !sha256_hex(&bytes).eq_ignore_ascii_case(&expected_digest) {
            return Err(
                "The downloaded Computer Helper did not match the published checksum; it was discarded."
                    .to_owned(),
            );
        }

        fs::create_dir_all(root)
            .map_err(|error| format!("Could not create {}: {error}", root.display()))?;
        let destination = root.join("Local Computer Helper.exe");
        let temp = root.join(".Local Computer Helper.exe.download");
        fs::write(&temp, &bytes)
            .map_err(|error| format!("Could not save the Computer Helper: {error}"))?;
        move_file_replace(&temp, &destination).map_err(|error| {
            let _ = fs::remove_file(&temp);
            format!("Could not install the Computer Helper: {error}")
        })?;
        Ok(destination)
    }
}

#[cfg(target_os = "windows")]
pub use windows_install::{
    INSTALLED_EXE_NAME, ensure_helper_downloaded, install, install_root, is_installed, uninstall,
};

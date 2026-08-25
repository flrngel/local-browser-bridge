use std::io;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore as _;
use subtle::ConstantTimeEq as _;
use tokio::fs;
#[cfg(target_os = "windows")]
use tokio::io::AsyncReadExt as _;
#[cfg(not(any(unix, target_os = "windows")))]
use tokio::io::AsyncWriteExt as _;

#[cfg(target_os = "windows")]
#[path = "token_windows.rs"]
mod windows_security;

const TOKEN_BYTES: usize = 32;
const TOKEN_ENCODED_BYTES: usize = 43;
const MIN_DISTINCT_TOKEN_BYTES: usize = 16;
const MANAGED_TOKEN_DIRECTORY: &str = ".local-browser-bridge";

#[cfg(unix)]
type TokenDirectory = std::fs::File;
#[cfg(target_os = "windows")]
type TokenDirectory = windows_security::TokenDirectory;
#[cfg(not(any(unix, target_os = "windows")))]
struct TokenDirectory;

pub fn create_token() -> String {
    loop {
        let mut bytes = [0_u8; TOKEN_BYTES];
        rand::rng().fill_bytes(&mut bytes);
        let token = URL_SAFE_NO_PAD.encode(bytes);
        if token_is_valid(&token) {
            return token;
        }
    }
}

pub fn token_is_valid(value: &str) -> bool {
    if value.len() != TOKEN_ENCODED_BYTES || value.bytes().any(|byte| byte.is_ascii_whitespace()) {
        return false;
    }
    let Ok(decoded) = URL_SAFE_NO_PAD.decode(value) else {
        return false;
    };
    if decoded.len() != TOKEN_BYTES || URL_SAFE_NO_PAD.encode(&decoded) != value {
        return false;
    }

    let mut seen = [false; 256];
    let distinct = decoded
        .iter()
        .filter(|byte| {
            let index = usize::from(**byte);
            let was_new = !seen[index];
            seen[index] = true;
            was_new
        })
        .count();
    distinct >= MIN_DISTINCT_TOKEN_BYTES
}

pub fn default_token_path() -> io::Result<PathBuf> {
    default_token_path_from_home(crate::home::home_dir())
}

fn default_token_path_from_home(home: Option<PathBuf>) -> io::Result<PathBuf> {
    let Some(home) = home.filter(|path| path.is_absolute()) else {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "could not resolve an absolute current-user profile directory; set LBB_TOKEN_PATH to a token file inside a pre-created private directory",
        ));
    };
    Ok(home.join(MANAGED_TOKEN_DIRECTORY).join("token"))
}

pub async fn load_or_create_token(path: &Path) -> io::Result<String> {
    let managed_token_path = default_token_path().ok();
    load_or_create_token_with_managed_path(path, managed_token_path.as_deref()).await
}

async fn load_or_create_token_with_managed_path(
    path: &Path,
    managed_token_path: Option<&Path>,
) -> io::Result<String> {
    let directory = prepare_token_parent(path, managed_token_path).await?;

    match load_valid_persisted_token(&directory, path).await {
        Ok(Some(token)) => return Ok(token),
        Ok(None) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let token = create_token();
    replace_token_file(&directory, path, &token).await?;
    Ok(token)
}

async fn prepare_token_parent(
    path: &Path,
    managed_token_path: Option<&Path>,
) -> io::Result<TokenDirectory> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if managed_token_path.is_some_and(|managed_path| path == managed_path) {
        prepare_managed_token_directory(parent).await
    } else {
        validate_private_token_directory(parent)
            .await
            .map_err(custom_parent_error)
    }
}

fn custom_parent_error(error: io::Error) -> io::Error {
    io::Error::new(
        error.kind(),
        format!(
            "create the custom token parent first as an ordinary private directory (current-user mode 0700 on Unix or a protected current-user-only DACL on Windows); the bridge did not create it or change its permissions: {error}"
        ),
    )
}

async fn prepare_managed_token_directory(path: &Path) -> io::Result<TokenDirectory> {
    match fs::symlink_metadata(path).await {
        Ok(_) => harden_managed_token_directory(path).await,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            create_private_token_directory(path).await
        }
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
async fn load_valid_persisted_token(
    directory: &TokenDirectory,
    path: &Path,
) -> io::Result<Option<String>> {
    use std::io::Read as _;

    let token_name = unix_child_name(path)?;
    let mut file = match unix_openat(
        directory,
        &token_name,
        libc::O_RDONLY | libc::O_NONBLOCK | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        None,
    ) {
        Ok(file) => file,
        Err(error) if error.raw_os_error() == Some(libc::ELOOP) => return Ok(None),
        Err(error) => return Err(error),
    };
    let metadata = file.metadata()?;
    if !metadata.file_type().is_file() {
        return Ok(None);
    }
    use std::os::unix::fs::MetadataExt as _;
    if metadata.nlink() != 1 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "refusing to use a multiply linked Unix token file",
        ));
    }
    if !private_file_permissions_are_valid(&metadata) {
        return Ok(None);
    }
    if metadata.len() > 128 {
        return Ok(None);
    }

    let mut contents = String::new();
    file.read_to_string(&mut contents)?;
    let value = contents.strip_suffix('\n').unwrap_or(&contents);
    if value.contains(['\r', '\n']) || !token_is_valid(value) {
        return Ok(None);
    }
    Ok(Some(value.to_owned()))
}

#[cfg(not(any(unix, target_os = "windows")))]
async fn load_valid_persisted_token(
    _directory: &TokenDirectory,
    path: &Path,
) -> io::Result<Option<String>> {
    let metadata = fs::symlink_metadata(path).await?;
    if !metadata.file_type().is_file() || !private_file_permissions_are_valid(&metadata) {
        return Ok(None);
    }
    if metadata.len() > 128 {
        return Ok(None);
    }

    let contents = fs::read_to_string(path).await?;
    let value = contents.strip_suffix('\n').unwrap_or(&contents);
    if value.contains(['\r', '\n']) || !token_is_valid(value) {
        return Ok(None);
    }
    Ok(Some(value.to_owned()))
}

#[cfg(target_os = "windows")]
async fn load_valid_persisted_token(
    directory: &TokenDirectory,
    path: &Path,
) -> io::Result<Option<String>> {
    let Some(file) = windows_security::open_private_token_file(directory, path)? else {
        return Ok(None);
    };
    let mut file = fs::File::from_std(file);
    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;
    windows_security::ensure_token_directory_bound(directory)?;
    let value = contents.strip_suffix('\n').unwrap_or(&contents);
    if value.contains(['\r', '\n']) || !token_is_valid(value) {
        return Ok(None);
    }
    Ok(Some(value.to_owned()))
}

#[cfg(unix)]
async fn replace_token_file(
    directory: &TokenDirectory,
    path: &Path,
    token: &str,
) -> io::Result<()> {
    use std::io::Write as _;

    let token_name = unix_child_name(path)?;
    let temporary_name = std::ffi::CString::new(format!(".lbb-token-{}.tmp", create_token()))
        .expect("generated token temporary name contains no NUL");
    let mut temporary_created = false;

    let result = (|| {
        let mut file = unix_openat(
            directory,
            &temporary_name,
            libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            Some(0o600),
        )?;
        temporary_created = true;
        file.write_all(format!("{token}\n").as_bytes())?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        unix_renameat(directory, &temporary_name, &token_name)?;
        temporary_created = false;
        directory.sync_all()?;
        verify_replaced_unix_token_file(directory, &token_name)
    })();
    if result.is_err() && temporary_created {
        let _ = unix_unlinkat(directory, &temporary_name);
    }
    result
}

#[cfg(target_os = "windows")]
async fn replace_token_file(
    directory: &TokenDirectory,
    path: &Path,
    token: &str,
) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temporary_path = parent.join(format!(".lbb-token-{}.tmp", create_token()));
    let mut file = windows_security::create_private_token_file(directory, &temporary_path)?;
    if let Err(error) = file.write_and_sync(format!("{token}\n").as_bytes()) {
        let cleanup = file.discard();
        return Err(windows_temporary_cleanup_error(error, cleanup));
    }

    if let Err(error) = windows_security::replace_token_file(directory, &mut file, path) {
        let cleanup = file.discard();
        return Err(windows_temporary_cleanup_error(error, cleanup));
    }

    // The rename is the commit boundary. Never attempt temporary-name cleanup after this point:
    // a later verification error cannot make reopening that old leaf safe.
    windows_security::verify_replaced_token_file(directory, &file, path)
}

#[cfg(target_os = "windows")]
fn windows_temporary_cleanup_error(operation: io::Error, cleanup: io::Result<()>) -> io::Error {
    match cleanup {
        Ok(()) => operation,
        Err(cleanup) => io::Error::new(
            cleanup.kind(),
            format!(
                "the Windows token operation failed and exact-handle temporary cleanup also failed: operation={operation}; cleanup={cleanup}"
            ),
        ),
    }
}

#[cfg(not(any(unix, target_os = "windows")))]
async fn replace_token_file(
    _directory: &TokenDirectory,
    path: &Path,
    token: &str,
) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temporary_path = parent.join(format!(".lbb-token-{}.tmp", create_token()));
    let result = async {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = options.open(&temporary_path).await?;
        file.write_all(format!("{token}\n").as_bytes()).await?;
        file.flush().await?;
        file.sync_all().await?;
        drop(file);
        fs::rename(&temporary_path, path).await
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path).await;
    }
    result
}

pub fn tokens_equal(actual: &str, expected: &str) -> bool {
    token_is_valid(actual)
        && token_is_valid(expected)
        && bool::from(actual.as_bytes().ct_eq(expected.as_bytes()))
}

#[cfg(unix)]
async fn create_private_token_directory(path: &Path) -> io::Result<TokenDirectory> {
    use std::os::unix::fs::DirBuilderExt as _;

    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700).create(path)?;
    harden_managed_token_directory(path).await
}

#[cfg(target_os = "windows")]
async fn create_private_token_directory(path: &Path) -> io::Result<TokenDirectory> {
    windows_security::create_private_token_directory(path)
}

#[cfg(not(any(unix, target_os = "windows")))]
async fn create_private_token_directory(path: &Path) -> io::Result<TokenDirectory> {
    fs::create_dir(path).await?;
    Ok(TokenDirectory)
}

#[cfg(unix)]
async fn harden_managed_token_directory(path: &Path) -> io::Result<TokenDirectory> {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    let directory = open_owned_unix_token_directory(path)?;
    let existing_mode = directory.metadata()?.mode() & 0o777;
    if existing_mode == 0o700 {
        validate_private_unix_directory_metadata(&directory.metadata()?)?;
        return Ok(directory);
    }
    if existing_mode & 0o700 != 0o700 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the managed token directory is missing required owner permissions; refusing to widen them",
        ));
    }
    directory.set_permissions(std::fs::Permissions::from_mode(0o700))?;
    validate_private_unix_directory_metadata(&directory.metadata()?)?;
    Ok(directory)
}

#[cfg(target_os = "windows")]
async fn harden_managed_token_directory(path: &Path) -> io::Result<TokenDirectory> {
    windows_security::harden_token_directory(path)
}

#[cfg(not(any(unix, target_os = "windows")))]
async fn harden_managed_token_directory(path: &Path) -> io::Result<TokenDirectory> {
    validate_private_token_directory(path).await
}

#[cfg(unix)]
async fn validate_private_token_directory(path: &Path) -> io::Result<TokenDirectory> {
    let directory = open_owned_unix_token_directory(path)?;
    validate_private_unix_directory_metadata(&directory.metadata()?)?;
    Ok(directory)
}

#[cfg(target_os = "windows")]
async fn validate_private_token_directory(path: &Path) -> io::Result<TokenDirectory> {
    windows_security::validate_private_token_directory(path)
}

#[cfg(not(any(unix, target_os = "windows")))]
async fn validate_private_token_directory(path: &Path) -> io::Result<TokenDirectory> {
    let metadata = fs::symlink_metadata(path).await?;
    if !metadata.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the custom token parent is not an ordinary directory",
        ));
    }
    Ok(TokenDirectory)
}

#[cfg(unix)]
fn private_file_permissions_are_valid(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    metadata.mode() & 0o777 == 0o600 && metadata.uid() == unsafe { libc::geteuid() }
}

#[cfg(not(any(unix, target_os = "windows")))]
fn private_file_permissions_are_valid(_metadata: &std::fs::Metadata) -> bool {
    true
}

#[cfg(unix)]
fn open_owned_unix_token_directory(path: &Path) -> io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt as _;

    let mut options = std::fs::OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW);
    let directory = options.open(path).map_err(|error| {
        if matches!(error.raw_os_error(), Some(code) if code == libc::ELOOP || code == libc::ENOTDIR)
        {
            io::Error::new(
                io::ErrorKind::PermissionDenied,
                "refusing to use a symlink as the token parent directory",
            )
        } else {
            error
        }
    })?;
    let metadata = directory.metadata()?;
    use std::os::unix::fs::MetadataExt as _;
    if !metadata.is_dir() || metadata.uid() != unsafe { libc::geteuid() } {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the token parent must be an ordinary directory owned by the current user",
        ));
    }
    Ok(directory)
}

#[cfg(unix)]
fn validate_private_unix_directory_metadata(metadata: &std::fs::Metadata) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt as _;

    if metadata.is_dir()
        && metadata.uid() == unsafe { libc::geteuid() }
        && metadata.mode() & 0o777 == 0o700
    {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the token parent must already be owned by the current user with mode 0700",
        ))
    }
}

#[cfg(unix)]
fn unix_child_name(path: &Path) -> io::Result<std::ffi::CString> {
    use std::os::unix::ffi::OsStrExt as _;

    let name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the token path must end with an ordinary file name",
        )
    })?;
    if name.as_bytes() == b"." || name.as_bytes() == b".." {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the token file name cannot be dot or dot-dot",
        ));
    }
    std::ffi::CString::new(name.as_bytes()).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "a Unix token file name cannot contain NUL",
        )
    })
}

#[cfg(unix)]
fn unix_openat(
    directory: &TokenDirectory,
    name: &std::ffi::CStr,
    flags: libc::c_int,
    mode: Option<libc::mode_t>,
) -> io::Result<std::fs::File> {
    use std::os::fd::{AsRawFd as _, FromRawFd as _};

    let descriptor = match mode {
        Some(mode) => unsafe {
            libc::openat(
                directory.as_raw_fd(),
                name.as_ptr(),
                flags,
                libc::c_uint::from(mode),
            )
        },
        None => unsafe { libc::openat(directory.as_raw_fd(), name.as_ptr(), flags) },
    };
    if descriptor < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(unsafe { std::fs::File::from_raw_fd(descriptor) })
    }
}

#[cfg(unix)]
fn unix_renameat(
    directory: &TokenDirectory,
    source: &std::ffi::CStr,
    destination: &std::ffi::CStr,
) -> io::Result<()> {
    use std::os::fd::AsRawFd as _;

    let result = unsafe {
        libc::renameat(
            directory.as_raw_fd(),
            source.as_ptr(),
            directory.as_raw_fd(),
            destination.as_ptr(),
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn unix_unlinkat(directory: &TokenDirectory, name: &std::ffi::CStr) -> io::Result<()> {
    use std::os::fd::AsRawFd as _;

    let result = unsafe { libc::unlinkat(directory.as_raw_fd(), name.as_ptr(), 0) };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
fn verify_replaced_unix_token_file(
    directory: &TokenDirectory,
    name: &std::ffi::CStr,
) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt as _;

    let file = unix_openat(
        directory,
        name,
        libc::O_RDONLY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
        None,
    )?;
    let metadata = file.metadata()?;
    if metadata.is_file()
        && metadata.nlink() == 1
        && metadata.len() <= 128
        && private_file_permissions_are_valid(&metadata)
    {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the replaced Unix token file did not retain a private single-link identity",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "windows")]
    const WINDOWS_SWAP_TEMP_MARKER: &str = "decoy-safe-temp-marker";
    #[cfg(target_os = "windows")]
    const WINDOWS_SWAP_TOKEN_MARKER: &str = "decoy-unsafe-token-marker";

    #[cfg(target_os = "windows")]
    #[derive(Clone, Copy)]
    enum WindowsSwapDecoy {
        Empty,
        SafeTemporaryMarker,
        MultiplyLinkedTokenMarker,
    }

    #[cfg(target_os = "windows")]
    struct WindowsJunctionSwapFixture {
        public_ancestor: PathBuf,
        decoy_ancestor: PathBuf,
        moved_ancestor: PathBuf,
        public_parent: PathBuf,
        original_parent: PathBuf,
        decoy_parent: PathBuf,
    }

    #[test]
    fn default_token_path_requires_an_absolute_user_profile() {
        for home in [None, Some(PathBuf::from("relative-profile"))] {
            let error = default_token_path_from_home(home)
                .expect_err("the default token path must never fall back to the working directory");
            assert_eq!(error.kind(), io::ErrorKind::NotFound);
            assert!(error.to_string().contains("LBB_TOKEN_PATH"));
        }

        #[cfg(unix)]
        let home = PathBuf::from("/Users/example");
        #[cfg(target_os = "windows")]
        let home = PathBuf::from(r"C:\Users\example");
        #[cfg(not(any(unix, target_os = "windows")))]
        let home = std::env::current_dir().expect("absolute current directory");

        assert_eq!(
            default_token_path_from_home(Some(home.clone())).expect("absolute profile path"),
            home.join(MANAGED_TOKEN_DIRECTORY).join("token")
        );
    }

    #[tokio::test]
    async fn creates_and_reuses_a_persisted_token() {
        let directory = tempfile::tempdir().expect("temp directory");
        let parent = directory.path().join(MANAGED_TOKEN_DIRECTORY);
        let path = parent.join("token");
        let first = load_or_create_managed_test_token(&path)
            .await
            .expect("create token");
        let second = load_or_create_managed_test_token(&path)
            .await
            .expect("reuse token");
        assert_eq!(first, second);
        assert_eq!(std::fs::read_to_string(&path).unwrap().trim(), first);
        assert_eq!(first.len(), TOKEN_ENCODED_BYTES);
        assert!(token_is_valid(&first));
        assert_private_test_permissions(&parent, 0o700);
        assert_private_test_permissions(&path, 0o600);
    }

    #[tokio::test]
    async fn rotates_empty_whitespace_malformed_and_weak_tokens() {
        let directory = tempfile::tempdir().expect("temp directory");
        let parent = private_test_parent(directory.path()).await;
        let path = parent.join("token");
        let weak = URL_SAFE_NO_PAD.encode([0_u8; TOKEN_BYTES]);
        let invalid_base64 = format!("{}!", "A".repeat(TOKEN_ENCODED_BYTES - 1));
        let wrong_decoded_length = "A".repeat(TOKEN_ENCODED_BYTES - 1);
        let invalid = [
            String::new(),
            "   \n".to_owned(),
            invalid_base64,
            wrong_decoded_length,
            weak,
        ];

        for contents in invalid {
            std::fs::write(&path, &contents).expect("write invalid token");
            set_private_test_permissions(&path);
            let token = load_or_create_token(&path).await.expect("rotate token");
            assert!(token_is_valid(&token));
            assert_ne!(token, contents.trim());
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn replaces_symlink_without_touching_its_target() {
        use std::os::unix::fs::symlink;

        let directory = tempfile::tempdir().expect("temp directory");
        let parent = private_test_parent(directory.path()).await;
        let target = directory.path().join("target");
        let path = parent.join("token");
        let target_token = create_token();
        std::fs::write(&target, format!("{target_token}\n")).expect("write target");
        symlink(&target, &path).expect("create symlink");

        let token = load_or_create_token(&path).await.expect("replace symlink");
        assert!(token_is_valid(&token));
        assert_ne!(token, target_token);
        assert!(
            !std::fs::symlink_metadata(&path)
                .expect("token metadata")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(&target)
                .expect("target remains")
                .trim(),
            target_token
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rotates_a_fifo_without_blocking_for_a_writer() {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt as _;

        let directory = tempfile::tempdir().expect("temp directory");
        let parent = private_test_parent(directory.path()).await;
        let path = parent.join("token");
        let fifo_path = CString::new(path.as_os_str().as_bytes()).expect("FIFO path has no NUL");
        assert_eq!(unsafe { libc::mkfifo(fifo_path.as_ptr(), 0o600) }, 0);

        let delayed_writer_path = path.clone();
        let delayed_writer = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(1));
            std::fs::OpenOptions::new()
                .write(true)
                .open(delayed_writer_path)
                .expect("open the FIFO or its safe replacement");
        });

        let started = std::time::Instant::now();
        let token = load_or_create_token(&path)
            .await
            .expect("replace a special token entry without waiting for a FIFO peer");
        let elapsed = started.elapsed();
        delayed_writer.join().expect("delayed writer thread");

        assert!(
            elapsed < std::time::Duration::from_millis(800),
            "token inspection blocked for a FIFO writer: {elapsed:?}"
        );
        assert!(token_is_valid(&token));
        assert!(
            std::fs::symlink_metadata(&path)
                .expect("replacement token metadata")
                .file_type()
                .is_file()
        );
        assert_eq!(
            std::fs::read_to_string(&path)
                .expect("replacement token contents")
                .trim(),
            token
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rotates_a_token_file_with_excessive_permissions() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("temp directory");
        let parent = private_test_parent(directory.path()).await;
        let path = parent.join("token");
        let original = create_token();
        std::fs::write(&path, format!("{original}\n")).expect("write token");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644))
            .expect("make token permissive");

        let replacement = load_or_create_token(&path).await.expect("rotate token");
        assert_ne!(replacement, original);
        assert!(token_is_valid(&replacement));
        let mode = std::fs::metadata(&path)
            .expect("token metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn rotates_a_windows_token_with_a_null_dacl() {
        let directory = tempfile::tempdir().expect("temp directory");
        let parent = private_test_parent(directory.path()).await;
        let path = parent.join("token");
        let original = create_token();
        std::fs::write(&path, format!("{original}\n")).expect("write token");
        windows_security::install_permissive_null_dacl_for_test(&path)
            .expect("install test null DACL");

        let replacement = load_or_create_token(&path)
            .await
            .expect("rotate permissive token");
        assert_ne!(replacement, original);
        assert!(token_is_valid(&replacement));
        assert!(
            windows_security::path_has_private_permissions_for_test(&path, false)
                .expect("inspect replacement DACL")
        );
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn rejects_a_windows_file_symlink_without_touching_its_target() {
        use std::os::windows::fs::symlink_file;

        let directory = tempfile::tempdir().expect("temp directory");
        let parent = private_test_parent(directory.path()).await;
        let target = directory.path().join("target");
        let path = parent.join("token");
        let target_token = create_token();
        std::fs::write(&target, format!("{target_token}\n")).expect("write target");
        match symlink_file(&target, &path) {
            Ok(()) => {}
            Err(error)
                if error.kind() == io::ErrorKind::PermissionDenied
                    || error.raw_os_error() == Some(1314) =>
            {
                eprintln!("skipping Windows symlink runtime assertion: {error}");
                return;
            }
            Err(error) => panic!("create Windows file symlink: {error}"),
        }

        let error = load_or_create_token(&path)
            .await
            .expect_err("reject Windows symlink");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(
            std::fs::symlink_metadata(&path)
                .expect("token metadata")
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            std::fs::read_to_string(&target)
                .expect("target remains")
                .trim(),
            target_token
        );
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn rejects_a_multiply_linked_windows_token_without_modifying_either_name() {
        let directory = tempfile::tempdir().expect("temp directory");
        let parent = private_test_parent(directory.path()).await;
        let target = directory.path().join("target");
        let path = parent.join("token");
        let target_token = create_token();
        std::fs::write(&target, format!("{target_token}\n")).expect("write target");
        std::fs::hard_link(&target, &path).expect("create token hard link");

        let error = load_or_create_token(&path)
            .await
            .expect_err("reject multiply linked Windows token");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            std::fs::read_to_string(&path)
                .expect("token link remains")
                .trim(),
            target_token
        );
        assert_eq!(
            std::fs::read_to_string(&target)
                .expect("target remains")
                .trim(),
            target_token
        );
    }

    #[tokio::test]
    async fn refuses_to_create_an_arbitrary_custom_parent() {
        let directory = tempfile::tempdir().expect("temp directory");
        let parent = directory.path().join("custom-token-parent");
        let path = parent.join("token");

        let error = load_or_create_token(&path)
            .await
            .expect_err("missing custom parent must fail closed");

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(
            error
                .to_string()
                .contains("create the custom token parent first")
        );
        assert!(
            error
                .to_string()
                .contains("did not create it or change its permissions")
        );
        assert!(!parent.exists());
        assert!(!path.exists());
    }

    #[tokio::test]
    async fn accepts_a_preconfigured_private_custom_parent_without_renaming_it() {
        let directory = tempfile::tempdir().expect("temp directory");
        let managed = private_test_parent(directory.path()).await;
        let custom = directory.path().join("custom-private");
        std::fs::rename(&managed, &custom).expect("rename private directory");
        let path = custom.join("token");

        let token = load_or_create_token(&path)
            .await
            .expect("use private custom parent");

        assert!(token_is_valid(&token));
        assert!(custom.is_dir());
        assert_private_test_permissions(&custom, 0o700);
        assert_private_test_permissions(&path, 0o600);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_a_permissive_custom_parent_without_chmod() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("temp directory");
        let parent = directory.path().join("custom-public");
        std::fs::create_dir(&parent).expect("create custom parent");
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755))
            .expect("make parent permissive");
        let path = parent.join("token");

        let error = load_or_create_token(&path)
            .await
            .expect_err("permissive custom parent must fail closed");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            std::fs::metadata(&parent)
                .expect("custom parent metadata")
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn an_arbitrary_same_named_unix_parent_is_still_custom_and_unchanged() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("temp directory");
        let parent = directory.path().join(MANAGED_TOKEN_DIRECTORY);
        std::fs::create_dir(&parent).expect("create same-named custom parent");
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755))
            .expect("make same-named parent permissive");
        let path = parent.join("token");

        let error = load_or_create_token(&path)
            .await
            .expect_err("name alone must not grant managed status");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            std::fs::metadata(&parent)
                .expect("same-named parent metadata")
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert!(!path.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn managed_unix_policy_narrows_excess_group_and_other_permissions() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("temp directory");
        let parent = directory.path().join(MANAGED_TOKEN_DIRECTORY);
        std::fs::create_dir(&parent).expect("create managed parent");
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o755))
            .expect("make managed parent permissive");

        let token = load_or_create_managed_test_token(&parent.join("token"))
            .await
            .expect("harden managed parent");

        assert!(token_is_valid(&token));
        assert_private_test_permissions(&parent, 0o700);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn managed_unix_policy_never_widens_missing_owner_permissions() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("temp directory");
        let parent = directory.path().join(MANAGED_TOKEN_DIRECTORY);
        std::fs::create_dir(&parent).expect("create managed parent");
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o500))
            .expect("remove owner write permission");

        let error = load_or_create_managed_test_token(&parent.join("token"))
            .await
            .expect_err("managed parent permissions must not be widened");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            std::fs::metadata(&parent)
                .expect("managed parent metadata")
                .permissions()
                .mode()
                & 0o777,
            0o500
        );
        assert!(!parent.join("token").exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_a_multiply_linked_unix_token_without_modifying_either_name() {
        use std::os::unix::fs::MetadataExt as _;

        let directory = tempfile::tempdir().expect("temp directory");
        let parent = private_test_parent(directory.path()).await;
        let target = directory.path().join("target");
        let path = parent.join("token");
        let original = create_token();
        std::fs::write(&target, format!("{original}\n")).expect("write token target");
        set_private_test_permissions(&target);
        std::fs::hard_link(&target, &path).expect("create token hard link");

        let error = load_or_create_token(&path)
            .await
            .expect_err("reject multiply linked token");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            std::fs::read_to_string(&path)
                .expect("token hard link remains")
                .trim(),
            original
        );
        assert_eq!(
            std::fs::read_to_string(&target)
                .expect("other hard link remains")
                .trim(),
            original
        );
        assert_eq!(
            std::fs::metadata(&path)
                .expect("hard-link metadata")
                .nlink(),
            2
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn unix_token_transaction_stays_bound_to_the_validated_parent_after_a_path_swap() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = tempfile::tempdir().expect("temp directory");
        let parent = private_test_parent(directory.path()).await;
        let path = parent.join("token");
        let original = load_or_create_token(&path).await.expect("create token");
        let capability = prepare_token_parent(&path, None)
            .await
            .expect("retain validated parent directory");

        let moved_parent = directory.path().join("validated-parent");
        std::fs::rename(&parent, &moved_parent).expect("move validated parent");
        std::fs::create_dir(&parent).expect("install replacement parent");
        std::fs::set_permissions(&parent, std::fs::Permissions::from_mode(0o700))
            .expect("make replacement parent private");
        let decoy = create_token();
        std::fs::write(&path, format!("{decoy}\n")).expect("write replacement-parent decoy");
        set_private_test_permissions(&path);

        assert_eq!(
            load_valid_persisted_token(&capability, &path)
                .await
                .expect("read through retained directory descriptor"),
            Some(original)
        );

        let replacement = create_token();
        replace_token_file(&capability, &path, &replacement)
            .await
            .expect("replace through retained directory descriptor");

        assert_eq!(
            std::fs::read_to_string(moved_parent.join("token"))
                .expect("read capability-bound replacement")
                .trim(),
            replacement
        );
        assert_eq!(
            std::fs::read_to_string(&path)
                .expect("read replacement-parent decoy")
                .trim(),
            decoy,
            "the substituted pathname must not redirect token replacement"
        );
        for token_parent in [&parent, &moved_parent] {
            assert!(
                std::fs::read_dir(token_parent)
                    .expect("list token parent")
                    .all(|entry| !entry
                        .expect("directory entry")
                        .file_name()
                        .to_string_lossy()
                        .starts_with(".lbb-token-")),
                "temporary token files must be cleaned up in the capability-bound directory"
            );
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn rejects_a_custom_parent_symlink_without_chmodding_its_target() {
        use std::os::unix::fs::{PermissionsExt as _, symlink};

        let directory = tempfile::tempdir().expect("temp directory");
        let target = directory.path().join("real-parent");
        let linked_parent = directory.path().join("linked-parent");
        std::fs::create_dir(&target).expect("create target directory");
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))
            .expect("make target permissive");
        symlink(&target, &linked_parent).expect("create parent symlink");

        let error = load_or_create_token(&linked_parent.join("token"))
            .await
            .expect_err("custom parent symlink must fail closed");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(
            std::fs::metadata(&target)
                .expect("target metadata")
                .permissions()
                .mode()
                & 0o777,
            0o755
        );
        assert!(
            std::fs::symlink_metadata(&linked_parent)
                .expect("symlink metadata")
                .file_type()
                .is_symlink()
        );
        assert!(!target.join("token").exists());
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn rejects_a_permissive_windows_custom_parent_without_rewriting_its_dacl() {
        let directory = tempfile::tempdir().expect("temp directory");
        let parent = directory.path().join("custom-public");
        std::fs::create_dir(&parent).expect("create custom parent");
        windows_security::install_permissive_null_dacl_for_test(&parent)
            .expect("install permissive parent DACL");
        let path = parent.join("token");

        let error = load_or_create_token(&path)
            .await
            .expect_err("permissive custom parent must fail closed");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(
            !windows_security::path_has_private_permissions_for_test(&parent, true)
                .expect("inspect unchanged custom parent DACL")
        );
        assert!(!path.exists());
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn an_arbitrary_same_named_windows_parent_is_still_custom_and_unchanged() {
        let directory = tempfile::tempdir().expect("temp directory");
        let parent = directory.path().join(MANAGED_TOKEN_DIRECTORY);
        std::fs::create_dir(&parent).expect("create same-named custom parent");
        windows_security::install_permissive_null_dacl_for_test(&parent)
            .expect("install permissive parent DACL");
        let path = parent.join("token");

        let error = load_or_create_token(&path)
            .await
            .expect_err("name alone must not grant managed status");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(
            !windows_security::path_has_private_permissions_for_test(&parent, true)
                .expect("inspect unchanged same-named parent DACL")
        );
        assert!(!path.exists());
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn managed_windows_policy_replaces_a_permissive_dacl_with_the_exact_private_dacl() {
        let directory = tempfile::tempdir().expect("temp directory");
        let parent = directory.path().join(MANAGED_TOKEN_DIRECTORY);
        drop(
            windows_security::create_private_token_directory(&parent)
                .expect("create a deterministic TokenUser-owned managed parent"),
        );
        windows_security::install_permissive_null_dacl_for_test(&parent)
            .expect("install permissive parent DACL");

        let token = load_or_create_managed_test_token(&parent.join("token"))
            .await
            .expect("harden managed parent");

        assert!(token_is_valid(&token));
        assert!(
            windows_security::path_has_private_permissions_for_test(&parent, true)
                .expect("inspect hardened managed parent DACL")
        );
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn rejects_a_windows_parent_reparse_point_without_touching_its_target() {
        use std::os::windows::fs::symlink_dir;

        let directory = tempfile::tempdir().expect("temp directory");
        let managed = private_test_parent(directory.path()).await;
        let target = directory.path().join("real-parent");
        std::fs::rename(&managed, &target).expect("rename private target directory");
        let linked_parent = directory.path().join("linked-parent");
        match symlink_dir(&target, &linked_parent) {
            Ok(()) => {}
            Err(error)
                if error.kind() == io::ErrorKind::PermissionDenied
                    || error.raw_os_error() == Some(1314) =>
            {
                eprintln!("skipping Windows directory symlink runtime assertion: {error}");
                return;
            }
            Err(error) => panic!("create Windows directory symlink: {error}"),
        }

        let error = load_or_create_token(&linked_parent.join("token"))
            .await
            .expect_err("parent reparse point must fail closed");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(
            std::fs::symlink_metadata(&linked_parent)
                .expect("linked parent metadata")
                .file_type()
                .is_symlink()
        );
        assert!(!target.join("token").exists());
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn windows_relative_read_does_not_follow_an_ancestor_path_swap() {
        let directory = tempfile::tempdir().expect("temp directory");
        let fixture = windows_junction_swap_fixture(directory.path()).await;
        let path = fixture.public_parent.join("token");
        let token = replace_token_for_windows_swap_test(&path).await;
        let capability = prepare_token_parent(&path, None)
            .await
            .expect("retain validated parent directory");
        let barrier = arm_windows_token_path_swap("read", &fixture, WindowsSwapDecoy::Empty);

        let error = load_valid_persisted_token(&capability, &path)
            .await
            .expect_err("the public path swap must be detected after the relative read");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        barrier.assert_consumed();
        assert_windows_path_swap_handoff(&fixture);
        assert!(!path.exists(), "the decoy parent must remain untouched");
        assert_eq!(
            std::fs::read_to_string(fixture.original_parent.join("token"))
                .expect("read token through the original parent identity")
                .trim(),
            token
        );
        drop(capability);
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn windows_relative_create_cleans_its_exact_handle_after_an_ancestor_path_swap() {
        let directory = tempfile::tempdir().expect("temp directory");
        let fixture = windows_junction_swap_fixture(directory.path()).await;
        let path = fixture.public_parent.join("token");
        let capability = prepare_token_parent(&path, None)
            .await
            .expect("retain validated parent directory");
        let temporary_path = fixture.public_parent.join("safe-temp");
        let barrier =
            arm_windows_token_path_swap("create", &fixture, WindowsSwapDecoy::SafeTemporaryMarker);

        let error = windows_security::create_private_token_file(&capability, &temporary_path)
            .expect_err("a changed public path must fail the post-create binding check");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        barrier.assert_consumed();
        assert_windows_path_swap_handoff(&fixture);
        assert_eq!(
            std::fs::read_to_string(&temporary_path).expect("read decoy temporary marker"),
            WINDOWS_SWAP_TEMP_MARKER,
            "relative creation or cleanup must not alter the same-named decoy leaf"
        );
        assert!(
            !fixture.original_parent.join("safe-temp").exists(),
            "post-create failure must delete the exact newly created handle"
        );
        drop(capability);
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn windows_private_temporary_validation_failures_always_delete_the_exact_handle() {
        for return_error in [false, true] {
            let directory = tempfile::tempdir().expect("temp directory");
            let parent = private_test_parent(directory.path()).await;
            let path = parent.join("token");
            let capability = prepare_token_parent(&path, None)
                .await
                .expect("retain validated parent directory");
            let temporary_path = parent.join("post-create-temp");
            let post_create_fault =
                windows_security::install_validation_fault_for_test("post_create", return_error);

            let _error = windows_security::create_private_token_file(&capability, &temporary_path)
                .expect_err("post-create validation failure must fail the operation");

            post_create_fault.assert_consumed();
            assert!(
                !temporary_path.exists(),
                "post-create validation failure left an empty private leaf"
            );

            let secret_path = parent.join("secret-temp");
            let mut source = windows_security::create_private_token_file(&capability, &secret_path)
                .expect("create private temporary capability");
            source
                .write_and_sync(format!("{}\n", create_token()).as_bytes())
                .expect("write secret-bearing temporary file");
            let pre_rename_fault =
                windows_security::install_validation_fault_for_test("pre_rename", return_error);

            let _error = windows_security::replace_token_file(&capability, &mut source, &path)
                .expect_err("pre-rename validation failure must fail before commit");
            pre_rename_fault.assert_consumed();
            source
                .discard()
                .expect("unconditionally delete the exact secret-bearing handle");

            assert!(
                !secret_path.exists(),
                "pre-rename validation failure leaked a secret-bearing temporary file"
            );
            assert!(!path.exists(), "pre-rename failure must not commit a token");
            drop(capability);
        }
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn windows_private_temporary_rejects_verify_before_commit() {
        let directory = tempfile::tempdir().expect("temp directory");
        let parent = private_test_parent(directory.path()).await;
        let path = parent.join("token");
        let capability = prepare_token_parent(&path, None)
            .await
            .expect("retain validated parent directory");
        let temporary_path = parent.join("uncommitted-temp");
        let mut source = windows_security::create_private_token_file(&capability, &temporary_path)
            .expect("create private temporary capability");
        source
            .write_and_sync(format!("{}\n", create_token()).as_bytes())
            .expect("write uncommitted temporary token");

        let error = windows_security::verify_replaced_token_file(&capability, &source, &path)
            .expect_err("verification before the rename commit boundary must fail");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        assert!(!path.exists(), "verification must not commit a token");
        source
            .discard()
            .expect("delete the still-uncommitted exact temporary handle");
        assert!(!temporary_path.exists());
        drop(capability);
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn windows_private_temporary_rejects_a_second_replace() {
        let directory = tempfile::tempdir().expect("temp directory");
        let parent = private_test_parent(directory.path()).await;
        let first_path = parent.join("token");
        let second_path = parent.join("second-token");
        let capability = prepare_token_parent(&first_path, None)
            .await
            .expect("retain validated parent directory");
        let temporary_path = parent.join("single-use-temp");
        let token = create_token();
        let mut source = windows_security::create_private_token_file(&capability, &temporary_path)
            .expect("create private temporary capability");
        source
            .write_and_sync(format!("{token}\n").as_bytes())
            .expect("write replacement token");
        windows_security::replace_token_file(&capability, &mut source, &first_path)
            .expect("commit the capability exactly once");

        let error = windows_security::replace_token_file(&capability, &mut source, &second_path)
            .expect_err("a committed capability must not be renamed a second time");

        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
        windows_security::verify_replaced_token_file(&capability, &source, &first_path)
            .expect("the first committed destination remains verifiable");
        assert_eq!(
            std::fs::read_to_string(&first_path)
                .expect("read first committed token")
                .trim(),
            token
        );
        assert!(
            !second_path.exists(),
            "a second destination must not appear"
        );
        drop(source);
        drop(capability);
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn windows_relative_target_check_and_rename_do_not_follow_ancestor_path_swaps() {
        for stage in ["target_check", "rename"] {
            let directory = tempfile::tempdir().expect("temp directory");
            let fixture = windows_junction_swap_fixture(directory.path()).await;
            let path = fixture.public_parent.join("token");
            let _initial = replace_token_for_windows_swap_test(&path).await;
            let capability = prepare_token_parent(&path, None)
                .await
                .expect("retain validated parent directory");
            let temporary_path = fixture.public_parent.join("safe-temp");
            let replacement = create_token();
            let mut source =
                windows_security::create_private_token_file(&capability, &temporary_path)
                    .expect("create retained private temporary handle");
            source
                .write_and_sync(format!("{replacement}\n").as_bytes())
                .expect("write and flush replacement token");
            let barrier = arm_windows_token_path_swap(
                stage,
                &fixture,
                WindowsSwapDecoy::MultiplyLinkedTokenMarker,
            );

            windows_security::replace_token_file(&capability, &mut source, &path)
                .expect("the handle-relative atomic rename must stay on the original directory");
            barrier.assert_consumed();
            assert_windows_path_swap_handoff(&fixture);
            let error = windows_security::verify_replaced_token_file(&capability, &source, &path)
                .expect_err("the public path replacement must fail the post-rename check");

            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
            let decoy_source = fixture.public_parent.join("decoy-token-source");
            assert_eq!(
                std::fs::read_to_string(&path).expect("read decoy token marker"),
                WINDOWS_SWAP_TOKEN_MARKER,
                "target inspection or rename must not alter the path-swapped decoy"
            );
            assert_eq!(
                std::fs::read_to_string(&decoy_source).expect("read linked decoy source"),
                WINDOWS_SWAP_TOKEN_MARKER
            );
            assert_eq!(
                windows_security::number_of_links_for_test(&path)
                    .expect("inspect linked decoy token"),
                2,
                "the deliberately unsafe path-based target must remain multiply linked"
            );
            assert_eq!(
                std::fs::read_to_string(fixture.original_parent.join("token"))
                    .expect("read token through the original parent identity")
                    .trim(),
                replacement
            );
            drop(source);
            drop(capability);
        }
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn windows_temporary_cleanup_deletes_the_exact_handle_after_an_ancestor_path_swap() {
        let directory = tempfile::tempdir().expect("temp directory");
        let fixture = windows_junction_swap_fixture(directory.path()).await;
        let path = fixture.public_parent.join("token");
        let capability = prepare_token_parent(&path, None)
            .await
            .expect("retain validated parent directory");
        let temporary_path = fixture.public_parent.join("safe-temp");
        let source = windows_security::create_private_token_file(&capability, &temporary_path)
            .expect("create retained private temporary handle");
        let barrier =
            arm_windows_token_path_swap("cleanup", &fixture, WindowsSwapDecoy::SafeTemporaryMarker);

        source
            .discard()
            .expect("delete the exact retained temporary handle");
        barrier.assert_consumed();
        assert_windows_path_swap_handoff(&fixture);

        assert_eq!(
            std::fs::read_to_string(&temporary_path).expect("read decoy temporary marker"),
            WINDOWS_SWAP_TEMP_MARKER,
            "exact-handle cleanup must not reopen or delete the path-swapped decoy"
        );
        assert!(
            !fixture.original_parent.join("safe-temp").exists(),
            "cleanup must delete the retained original rather than reopen a leaf"
        );
        drop(capability);
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn windows_token_child_names_reject_win32_namespace_ambiguity() {
        let directory = tempfile::tempdir().expect("temp directory");
        let parent = private_test_parent(directory.path()).await;
        let capability = prepare_token_parent(&parent.join("token"), None)
            .await
            .expect("retain validated parent directory");

        for name in [
            "token:stream",
            "CON",
            "com1.txt",
            "LPT².log",
            "trailing.",
            "trailing ",
            "space .txt",
            "question?.txt",
            "control\u{1}.txt",
        ] {
            let error =
                windows_security::create_private_token_file(&capability, &parent.join(name))
                    .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput, "name={name:?}");
        }
        drop(capability);
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn windows_handle_relative_rename_accepts_a_one_character_token_leaf() {
        let directory = tempfile::tempdir().expect("temp directory");
        let parent = private_test_parent(directory.path()).await;
        let path = parent.join("x");

        let token = load_or_create_token(&path)
            .await
            .expect("create one-character custom token leaf");

        assert_eq!(
            std::fs::read_to_string(&path)
                .expect("read one-character token leaf")
                .trim(),
            token
        );
    }

    #[cfg(target_os = "windows")]
    #[tokio::test]
    async fn windows_handle_relative_lookup_preserves_case_distinct_siblings() {
        let directory = tempfile::tempdir().expect("temp directory");
        let parent = private_test_parent(directory.path()).await;
        windows_security::enable_case_sensitive_directory_for_test(&parent)
            .expect("enable case-sensitive lookup on an empty private directory");
        let uppercase = parent.join("TOKEN");
        let uppercase_marker = "case-distinct-decoy";
        std::fs::write(&uppercase, uppercase_marker).expect("write uppercase sibling");
        let lowercase = parent.join("token");

        let token = load_or_create_token(&lowercase)
            .await
            .expect("create the exact lowercase token sibling");
        let reused = load_or_create_token(&lowercase)
            .await
            .expect("reuse the exact lowercase token sibling");

        assert_eq!(reused, token);
        assert_eq!(
            std::fs::read_to_string(&uppercase).expect("read uppercase sibling"),
            uppercase_marker,
            "case-insensitive native lookup must not select the uppercase decoy"
        );
        let mut names: Vec<_> = std::fs::read_dir(&parent)
            .expect("enumerate case-sensitive token parent")
            .map(|entry| entry.expect("directory entry").file_name())
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                std::ffi::OsString::from("TOKEN"),
                std::ffi::OsString::from("token")
            ],
            "both case-distinct leaves must coexist"
        );
    }

    #[cfg(target_os = "windows")]
    async fn replace_token_for_windows_swap_test(path: &Path) -> String {
        let token = create_token();
        let parent = path.parent().expect("test token parent");
        let capability = prepare_token_parent(path, None)
            .await
            .expect("retain private token directory");
        replace_token_file(&capability, path, &token)
            .await
            .expect("seed private token");
        drop(capability);
        assert!(parent.is_dir());
        token
    }

    #[cfg(target_os = "windows")]
    fn arm_windows_token_path_swap(
        stage: &'static str,
        fixture: &WindowsJunctionSwapFixture,
        decoy: WindowsSwapDecoy,
    ) -> windows_security::TestBarrierGuard {
        match decoy {
            WindowsSwapDecoy::Empty => {}
            WindowsSwapDecoy::SafeTemporaryMarker => {
                std::fs::write(
                    fixture.decoy_parent.join("safe-temp"),
                    WINDOWS_SWAP_TEMP_MARKER,
                )
                .expect("seed same-named decoy temporary marker");
            }
            WindowsSwapDecoy::MultiplyLinkedTokenMarker => {
                let source = fixture.decoy_parent.join("decoy-token-source");
                std::fs::write(&source, WINDOWS_SWAP_TOKEN_MARKER)
                    .expect("seed unsafe decoy token source");
                std::fs::hard_link(&source, fixture.decoy_parent.join("token"))
                    .expect("create multiply linked path-based token decoy");
            }
        }
        assert_windows_junction_binding(&fixture.public_ancestor, &fixture.original_parent);
        assert_windows_junction_binding(&fixture.decoy_ancestor, &fixture.decoy_parent);
        assert!(
            !windows_security::resolved_directory_paths_have_same_identity_for_test(
                &fixture.original_parent,
                &fixture.decoy_parent,
            )
            .expect("compare original and decoy parent identities"),
            "the original and decoy token parents must have distinct file identities"
        );
        let public_ancestor = fixture.public_ancestor.clone();
        let decoy_ancestor = fixture.decoy_ancestor.clone();
        let moved_ancestor = fixture.moved_ancestor.clone();
        windows_security::install_test_barrier(stage, move || {
            std::fs::rename(&public_ancestor, &moved_ancestor)
                .expect("move the original token-path ancestor junction aside");
            std::fs::rename(&decoy_ancestor, &public_ancestor)
                .expect("move the prebuilt decoy junction into the public token path");
        })
    }

    #[cfg(target_os = "windows")]
    async fn windows_junction_swap_fixture(root: &Path) -> WindowsJunctionSwapFixture {
        let original_root = root.join("original-root");
        std::fs::create_dir(&original_root).expect("create original junction target");
        let original_parent = private_test_parent(&original_root).await;
        let decoy_root = root.join("decoy-root");
        std::fs::create_dir(&decoy_root).expect("create decoy junction target");
        let decoy_parent = private_test_parent(&decoy_root).await;
        let public_ancestor = root.join("public-ancestor");
        windows_security::create_directory_junction_for_test(&public_ancestor, &original_root)
            .expect("create replaceable token-path ancestor junction");
        let decoy_ancestor = root.join("decoy-ancestor");
        windows_security::create_directory_junction_for_test(&decoy_ancestor, &decoy_root)
            .expect("create prebuilt decoy ancestor junction");
        let moved_ancestor = root.join("moved-ancestor");
        let public_parent = public_ancestor.join(MANAGED_TOKEN_DIRECTORY);
        assert!(
            public_parent.is_dir(),
            "the original ancestor junction must expose the private token parent"
        );
        WindowsJunctionSwapFixture {
            public_ancestor,
            decoy_ancestor,
            moved_ancestor,
            public_parent,
            original_parent,
            decoy_parent,
        }
    }

    #[cfg(target_os = "windows")]
    fn assert_windows_junction_binding(junction: &Path, expected_parent: &Path) {
        assert!(
            windows_security::path_is_mount_point_for_test(junction)
                .expect("inspect no-follow ancestor reparse tag"),
            "the swap fixture must use a local mount-point junction"
        );
        assert!(
            windows_security::resolved_directory_paths_have_same_identity_for_test(
                &junction.join(MANAGED_TOKEN_DIRECTORY),
                expected_parent,
            )
            .expect("compare resolved junction target identity"),
            "the ancestor junction must resolve to the intended token parent identity"
        );
    }

    #[cfg(target_os = "windows")]
    fn assert_windows_path_swap_handoff(fixture: &WindowsJunctionSwapFixture) {
        assert_windows_junction_binding(&fixture.moved_ancestor, &fixture.original_parent);
        assert_windows_junction_binding(&fixture.public_ancestor, &fixture.decoy_parent);
    }

    async fn private_test_parent(root: &Path) -> std::path::PathBuf {
        let parent = root.join(MANAGED_TOKEN_DIRECTORY);
        prepare_managed_token_directory(&parent)
            .await
            .expect("prepare private test parent");
        parent
    }

    async fn load_or_create_managed_test_token(path: &Path) -> io::Result<String> {
        load_or_create_token_with_managed_path(path, Some(path)).await
    }

    #[cfg(unix)]
    fn set_private_test_permissions(path: &Path) {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .expect("set token permissions");
    }

    #[cfg(target_os = "windows")]
    fn set_private_test_permissions(path: &Path) {
        windows_security::harden_file_for_test(path).expect("set private Windows token DACL");
    }

    #[cfg(not(any(unix, target_os = "windows")))]
    fn set_private_test_permissions(_path: &Path) {}

    #[cfg(unix)]
    fn assert_private_test_permissions(path: &Path, expected: u32) {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = std::fs::metadata(path)
            .expect("private path metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, expected);
    }

    #[cfg(target_os = "windows")]
    fn assert_private_test_permissions(path: &Path, expected: u32) {
        let directory = expected == 0o700;
        assert!(
            windows_security::path_has_private_permissions_for_test(path, directory)
                .expect("inspect private Windows path DACL")
        );
    }

    #[cfg(not(any(unix, target_os = "windows")))]
    fn assert_private_test_permissions(_path: &Path, _expected: u32) {}

    #[test]
    fn accepts_only_canonical_high_entropy_token_shapes() {
        let token = create_token();
        assert!(token_is_valid(&token));
        assert!(!token_is_valid(""));
        assert!(!token_is_valid("token"));
        assert!(!token_is_valid(&format!(" {token}")));
        assert!(!token_is_valid(&"A".repeat(TOKEN_ENCODED_BYTES - 1)));
        assert!(!token_is_valid(&format!(
            "{}!",
            "A".repeat(TOKEN_ENCODED_BYTES - 1)
        )));
        assert!(!token_is_valid(
            &URL_SAFE_NO_PAD.encode([0_u8; TOKEN_BYTES])
        ));
    }

    #[test]
    fn compares_tokens_without_accepting_mismatches() {
        let first = create_token();
        let second = create_token();
        assert!(tokens_equal(&first, &first));
        assert!(!tokens_equal(&first, &second));
        assert!(!tokens_equal("abc", "abc"));
        assert!(!tokens_equal("", ""));
    }
}

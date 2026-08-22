use std::io;
use std::path::{Path, PathBuf};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore as _;
use subtle::ConstantTimeEq as _;
use tokio::fs;
#[cfg(any(unix, target_os = "windows"))]
use tokio::io::AsyncReadExt as _;
use tokio::io::AsyncWriteExt as _;

#[cfg(target_os = "windows")]
#[path = "token_windows.rs"]
mod windows_security;

const TOKEN_BYTES: usize = 32;
const TOKEN_ENCODED_BYTES: usize = 43;
const MIN_DISTINCT_TOKEN_BYTES: usize = 16;
const MANAGED_TOKEN_DIRECTORY: &str = ".local-browser-bridge";

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

pub fn default_token_path() -> PathBuf {
    crate::home::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(MANAGED_TOKEN_DIRECTORY)
        .join("token")
}

pub async fn load_or_create_token(path: &Path) -> io::Result<String> {
    load_or_create_token_with_managed_path(path, &default_token_path()).await
}

async fn load_or_create_token_with_managed_path(
    path: &Path,
    managed_token_path: &Path,
) -> io::Result<String> {
    prepare_token_parent(path, managed_token_path).await?;

    match load_valid_persisted_token(path).await {
        Ok(Some(token)) => return Ok(token),
        Ok(None) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let token = create_token();
    replace_token_file(path, &token).await?;
    Ok(token)
}

async fn prepare_token_parent(path: &Path, managed_token_path: &Path) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if path == managed_token_path {
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

async fn prepare_managed_token_directory(path: &Path) -> io::Result<()> {
    match fs::symlink_metadata(path).await {
        Ok(_) => harden_managed_token_directory(path).await,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            create_private_token_directory(path).await
        }
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
async fn load_valid_persisted_token(path: &Path) -> io::Result<Option<String>> {
    let mut options = fs::OpenOptions::new();
    options.read(true).custom_flags(libc::O_NOFOLLOW);
    let mut file = match options.open(path).await {
        Ok(file) => file,
        Err(error) if error.raw_os_error() == Some(libc::ELOOP) => return Ok(None),
        Err(error) => return Err(error),
    };
    let metadata = file.metadata().await?;
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
    file.read_to_string(&mut contents).await?;
    let value = contents.strip_suffix('\n').unwrap_or(&contents);
    if value.contains(['\r', '\n']) || !token_is_valid(value) {
        return Ok(None);
    }
    Ok(Some(value.to_owned()))
}

#[cfg(not(any(unix, target_os = "windows")))]
async fn load_valid_persisted_token(path: &Path) -> io::Result<Option<String>> {
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
async fn load_valid_persisted_token(path: &Path) -> io::Result<Option<String>> {
    let Some(file) = windows_security::open_private_token_file(path)? else {
        return Ok(None);
    };
    let mut file = fs::File::from_std(file);
    let mut contents = String::new();
    file.read_to_string(&mut contents).await?;
    let value = contents.strip_suffix('\n').unwrap_or(&contents);
    if value.contains(['\r', '\n']) || !token_is_valid(value) {
        return Ok(None);
    }
    Ok(Some(value.to_owned()))
}

async fn replace_token_file(path: &Path, token: &str) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temporary_path = parent.join(format!(".lbb-token-{}.tmp", create_token()));

    let result = async {
        let mut file = open_private_temporary_file(&temporary_path).await?;
        file.write_all(format!("{token}\n").as_bytes()).await?;
        file.flush().await?;
        file.sync_all().await?;
        drop(file);
        replace_path(&temporary_path, path).await?;
        verify_replaced_token_file(path)
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&temporary_path).await;
    }
    result
}

#[cfg(unix)]
async fn open_private_temporary_file(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true).mode(0o600);
    options.open(path).await
}

#[cfg(target_os = "windows")]
async fn open_private_temporary_file(path: &Path) -> io::Result<fs::File> {
    windows_security::create_private_token_file(path).map(fs::File::from_std)
}

#[cfg(not(any(unix, target_os = "windows")))]
async fn open_private_temporary_file(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    options.open(path).await
}

#[cfg(target_os = "windows")]
fn verify_replaced_token_file(path: &Path) -> io::Result<()> {
    if windows_security::token_path_has_private_permissions(path)? {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the replaced token file did not retain a private Windows DACL",
        ))
    }
}

#[cfg(not(target_os = "windows"))]
fn verify_replaced_token_file(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub fn tokens_equal(actual: &str, expected: &str) -> bool {
    token_is_valid(actual)
        && token_is_valid(expected)
        && bool::from(actual.as_bytes().ct_eq(expected.as_bytes()))
}

#[cfg(unix)]
async fn replace_path(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination).await
}

#[cfg(target_os = "windows")]
async fn replace_path(source: &Path, destination: &Path) -> io::Result<()> {
    windows_security::replace_token_file(source, destination)
}

#[cfg(not(any(unix, target_os = "windows")))]
async fn replace_path(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination).await
}

#[cfg(unix)]
async fn create_private_token_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::DirBuilderExt as _;

    let mut builder = std::fs::DirBuilder::new();
    builder.mode(0o700).create(path)?;
    harden_managed_token_directory(path).await
}

#[cfg(target_os = "windows")]
async fn create_private_token_directory(path: &Path) -> io::Result<()> {
    windows_security::create_private_token_directory(path)
}

#[cfg(not(any(unix, target_os = "windows")))]
async fn create_private_token_directory(path: &Path) -> io::Result<()> {
    fs::create_dir(path).await
}

#[cfg(unix)]
async fn harden_managed_token_directory(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    let directory = open_owned_unix_token_directory(path)?;
    let existing_mode = directory.metadata()?.mode() & 0o777;
    if existing_mode == 0o700 {
        return validate_private_unix_directory_metadata(&directory.metadata()?);
    }
    if existing_mode & 0o700 != 0o700 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the managed token directory is missing required owner permissions; refusing to widen them",
        ));
    }
    directory.set_permissions(std::fs::Permissions::from_mode(0o700))?;
    validate_private_unix_directory_metadata(&directory.metadata()?)
}

#[cfg(target_os = "windows")]
async fn harden_managed_token_directory(path: &Path) -> io::Result<()> {
    windows_security::harden_token_directory(path)
}

#[cfg(not(any(unix, target_os = "windows")))]
async fn harden_managed_token_directory(path: &Path) -> io::Result<()> {
    validate_private_token_directory(path).await
}

#[cfg(unix)]
async fn validate_private_token_directory(path: &Path) -> io::Result<()> {
    let directory = open_owned_unix_token_directory(path)?;
    validate_private_unix_directory_metadata(&directory.metadata()?)
}

#[cfg(target_os = "windows")]
async fn validate_private_token_directory(path: &Path) -> io::Result<()> {
    windows_security::validate_private_token_directory(path)
}

#[cfg(not(any(unix, target_os = "windows")))]
async fn validate_private_token_directory(path: &Path) -> io::Result<()> {
    let metadata = fs::symlink_metadata(path).await?;
    if !metadata.file_type().is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "the custom token parent is not an ordinary directory",
        ));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

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
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
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
        std::fs::create_dir(&parent).expect("create managed parent");
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
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
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

    async fn private_test_parent(root: &Path) -> std::path::PathBuf {
        let parent = root.join(MANAGED_TOKEN_DIRECTORY);
        prepare_managed_token_directory(&parent)
            .await
            .expect("prepare private test parent");
        parent
    }

    async fn load_or_create_managed_test_token(path: &Path) -> io::Result<String> {
        load_or_create_token_with_managed_path(path, path).await
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

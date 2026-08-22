use std::io;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore as _;
use subtle::ConstantTimeEq as _;
use tokio::fs;
#[cfg(target_os = "windows")]
use tokio::io::AsyncReadExt as _;
use tokio::io::AsyncWriteExt as _;

#[cfg(target_os = "windows")]
#[path = "token_windows.rs"]
mod windows_security;

const TOKEN_BYTES: usize = 32;
const TOKEN_ENCODED_BYTES: usize = 43;
const MIN_DISTINCT_TOKEN_BYTES: usize = 16;

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

pub async fn load_or_create_token(path: &Path) -> io::Result<String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).await?;
        set_private_directory_permissions(parent).await?;
    }

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

#[cfg(not(target_os = "windows"))]
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
async fn set_private_directory_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await
}

#[cfg(target_os = "windows")]
async fn set_private_directory_permissions(path: &Path) -> io::Result<()> {
    windows_security::harden_token_directory(path)
}

#[cfg(not(any(unix, target_os = "windows")))]
async fn set_private_directory_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn private_file_permissions_are_valid(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o777 == 0o600
}

#[cfg(not(any(unix, target_os = "windows")))]
fn private_file_permissions_are_valid(_metadata: &std::fs::Metadata) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_and_reuses_a_persisted_token() {
        let directory = tempfile::tempdir().expect("temp directory");
        let parent = directory.path().join("nested");
        let path = parent.join("token");
        let first = load_or_create_token(&path).await.expect("create token");
        let second = load_or_create_token(&path).await.expect("reuse token");
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
        let path = directory.path().join("token");
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
        let target = directory.path().join("target");
        let path = directory.path().join("token");
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
        let path = directory.path().join("token");
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
        let path = directory.path().join("token");
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
        let target = directory.path().join("target");
        let path = directory.path().join("token");
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
        let target = directory.path().join("target");
        let path = directory.path().join("token");
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

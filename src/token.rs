use std::io;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore as _;
use subtle::ConstantTimeEq as _;
use tokio::fs;
use tokio::io::AsyncWriteExt as _;

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

async fn replace_token_file(path: &Path, token: &str) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let temporary_path = parent.join(format!(".lbb-token-{}.tmp", create_token()));
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    set_private_file_options(&mut options);

    let result = async {
        let mut file = options.open(&temporary_path).await?;
        file.write_all(format!("{token}\n").as_bytes()).await?;
        file.flush().await?;
        file.sync_all().await?;
        drop(file);
        replace_path(&temporary_path, path).await
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
async fn replace_path(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination).await
}

#[cfg(not(unix))]
async fn replace_path(source: &Path, destination: &Path) -> io::Result<()> {
    match fs::remove_file(destination).await {
        Ok(()) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::rename(source, destination).await
}

#[cfg(unix)]
fn set_private_file_options(options: &mut fs::OpenOptions) {
    options.mode(0o600);
}

#[cfg(not(unix))]
fn set_private_file_options(_options: &mut fs::OpenOptions) {}

#[cfg(unix)]
async fn set_private_directory_permissions(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).await
}

#[cfg(not(unix))]
async fn set_private_directory_permissions(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn private_file_permissions_are_valid(metadata: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o777 == 0o600
}

#[cfg(not(unix))]
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

    #[cfg(unix)]
    fn set_private_test_permissions(path: &Path) {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
            .expect("set token permissions");
    }

    #[cfg(not(unix))]
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

    #[cfg(not(unix))]
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

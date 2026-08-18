use std::io;
use std::path::Path;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use rand::RngCore as _;
use subtle::ConstantTimeEq as _;
use tokio::fs;
use tokio::io::AsyncWriteExt as _;

pub fn create_token() -> String {
    let mut bytes = [0_u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub async fn load_or_create_token(path: &Path) -> io::Result<String> {
    match fs::read_to_string(path).await {
        Ok(value) => return Ok(value.trim().to_owned()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).await?;
        set_private_directory_permissions(parent).await?;
    }

    let token = create_token();
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    set_private_file_options(&mut options);

    match options.open(path).await {
        Ok(mut file) => {
            file.write_all(format!("{token}\n").as_bytes()).await?;
            file.flush().await?;
            Ok(token)
        }
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
            Ok(fs::read_to_string(path).await?.trim().to_owned())
        }
        Err(error) => Err(error),
    }
}

pub fn tokens_equal(actual: &str, expected: &str) -> bool {
    actual.as_bytes().ct_eq(expected.as_bytes()).into()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_and_reuses_a_persisted_token() {
        let directory = tempfile::tempdir().expect("temp directory");
        let path = directory.path().join("nested").join("token");
        let first = load_or_create_token(&path).await.expect("create token");
        let second = load_or_create_token(&path).await.expect("reuse token");
        assert_eq!(first, second);
        assert_eq!(std::fs::read_to_string(path).unwrap().trim(), first);
        assert!(first.len() >= 40);
    }

    #[test]
    fn compares_tokens_without_accepting_mismatches() {
        assert!(tokens_equal("abc", "abc"));
        assert!(!tokens_equal("abc", "abd"));
        assert!(!tokens_equal("abc", "abcd"));
    }
}

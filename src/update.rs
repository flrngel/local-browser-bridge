use std::collections::HashSet;
use std::time::Duration;

use reqwest::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use crate::VERSION;

pub const RELEASES_URL: &str = "https://github.com/flrngel/local-browser-bridge/releases";
const LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/flrngel/local-browser-bridge/releases/latest";
const RELEASE_TAG_PREFIX: &str = "https://github.com/flrngel/local-browser-bridge/releases/tag/";
const MAX_RESPONSE_BYTES: usize = 512 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum UpdateState {
    Checking,
    UpToDate,
    Development,
    Available,
    Error,
    Disabled,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub status: UpdateState,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub release_url: Option<String>,
    pub checked_at: Option<String>,
    pub message: String,
}

impl UpdateStatus {
    pub fn checking() -> Self {
        Self {
            status: UpdateState::Checking,
            current_version: VERSION.to_owned(),
            latest_version: None,
            release_url: None,
            checked_at: None,
            message: "Checking the official GitHub release metadata. No files will be downloaded."
                .to_owned(),
        }
    }

    pub fn disabled() -> Self {
        Self {
            status: UpdateState::Disabled,
            current_version: VERSION.to_owned(),
            latest_version: None,
            release_url: Some(RELEASES_URL.to_owned()),
            checked_at: None,
            message: "Automatic update checks are disabled. No network request was made."
                .to_owned(),
        }
    }

    fn failed(message: impl Into<String>) -> Self {
        Self {
            status: UpdateState::Error,
            current_version: VERSION.to_owned(),
            latest_version: None,
            release_url: Some(RELEASES_URL.to_owned()),
            checked_at: Some(now_iso()),
            message: message.into(),
        }
    }
}

impl Default for UpdateStatus {
    fn default() -> Self {
        Self::disabled()
    }
}

#[derive(Deserialize)]
struct LatestRelease {
    tag_name: String,
    html_url: String,
    draft: bool,
    prerelease: bool,
    immutable: bool,
    assets: Vec<ReleaseAsset>,
}

#[derive(Deserialize)]
struct ReleaseAsset {
    name: String,
    size: u64,
    state: String,
    digest: Option<String>,
}

pub async fn check_for_update() -> UpdateStatus {
    check_for_update_at(LATEST_RELEASE_API, VERSION).await
}

async fn check_for_update_at(api_url: &str, current_version: &str) -> UpdateStatus {
    let client = match Client::builder()
        .user_agent(format!("local-browser-bridge/{VERSION} update-check"))
        .timeout(Duration::from_secs(6))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            return UpdateStatus::failed(
                "The update checker could not start. No files were downloaded.",
            );
        }
    };

    let mut response = match client
        .get(api_url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2026-03-10")
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            return UpdateStatus::failed(
                "Could not reach the official GitHub release API. No files were downloaded.",
            );
        }
    };

    if !response.status().is_success() {
        return UpdateStatus::failed(format!(
            "GitHub returned HTTP {} while checking release metadata. No files were downloaded.",
            response.status().as_u16()
        ));
    }
    if response
        .content_length()
        .is_some_and(|length| length > MAX_RESPONSE_BYTES as u64)
    {
        return UpdateStatus::failed(
            "GitHub returned oversized release metadata; it was rejected.",
        );
    }

    let mut bytes = Vec::with_capacity(
        response
            .content_length()
            .unwrap_or(0)
            .min(MAX_RESPONSE_BYTES as u64) as usize,
    );
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) if chunk.len() <= MAX_RESPONSE_BYTES.saturating_sub(bytes.len()) => {
                bytes.extend_from_slice(&chunk);
            }
            Ok(Some(_)) => {
                return UpdateStatus::failed(
                    "GitHub returned oversized release metadata; it was rejected.",
                );
            }
            Ok(None) => break,
            Err(_) => {
                return UpdateStatus::failed(
                    "GitHub release metadata could not be read. No files were downloaded.",
                );
            }
        }
    }
    match parse_release(&bytes, current_version) {
        Ok(status) => status,
        Err(message) => UpdateStatus::failed(message),
    }
}

fn parse_release(bytes: &[u8], current_version: &str) -> Result<UpdateStatus, String> {
    let release: LatestRelease = serde_json::from_slice(bytes)
        .map_err(|_| "GitHub returned invalid release metadata; it was rejected.".to_owned())?;
    if release.draft || release.prerelease {
        return Err("GitHub returned a non-stable release; it was ignored.".to_owned());
    }
    if !release.immutable {
        return Err("GitHub returned a mutable release; it was ignored.".to_owned());
    }
    let latest_text = release
        .tag_name
        .strip_prefix('v')
        .ok_or_else(|| "GitHub returned a release tag without the required v prefix.".to_owned())?;
    let expected_url = format!("{RELEASE_TAG_PREFIX}{}", release.tag_name);
    if release.html_url != expected_url {
        return Err("GitHub returned an unexpected release link; it was rejected.".to_owned());
    }

    let current = Version::parse(current_version)
        .map_err(|_| "The installed version is not valid semantic versioning.".to_owned())?;
    let latest = Version::parse(latest_text)
        .map_err(|_| "GitHub returned an invalid release version; it was rejected.".to_owned())?;
    if !latest.pre.is_empty()
        || !latest.build.is_empty()
        || release.tag_name != format!("v{latest}")
    {
        return Err(
            "GitHub returned a non-canonical stable release tag; it was ignored.".to_owned(),
        );
    }
    validate_release_assets(&release.assets, &latest)?;
    let (status, message) = match current.cmp(&latest) {
        std::cmp::Ordering::Less => (
            UpdateState::Available,
            format!(
                "Version {latest} is available. Review and download it from the official GitHub release."
            ),
        ),
        std::cmp::Ordering::Equal => (
            UpdateState::UpToDate,
            format!("Version {current} is the latest stable release."),
        ),
        std::cmp::Ordering::Greater => (
            UpdateState::Development,
            format!(
                "Version {current} is a development build ahead of the latest public stable release, version {latest}. Review the public release at {}.",
                release.html_url
            ),
        ),
    };
    Ok(UpdateStatus {
        status,
        current_version: current.to_string(),
        latest_version: Some(latest.to_string()),
        release_url: Some(release.html_url),
        checked_at: Some(now_iso()),
        message,
    })
}

fn validate_release_assets(assets: &[ReleaseAsset], version: &Version) -> Result<(), String> {
    let expected = [
        format!("local-browser-bridge-extension-v{version}.zip"),
        format!("local-browser-bridge-v{version}-macos-universal.tar.gz"),
        format!("local-browser-bridge-v{version}-windows-x86_64.exe"),
        format!("local-computer-helper-v{version}-windows-x86_64.exe"),
        "SHA256SUMS.txt".to_owned(),
    ];
    if assets.len() != expected.len() {
        return Err(
            "GitHub returned an incomplete or unexpected release asset set; it was rejected."
                .to_owned(),
        );
    }

    let mut actual = HashSet::with_capacity(assets.len());
    for asset in assets {
        if !actual.insert(asset.name.as_str())
            || asset.size == 0
            || asset.state != "uploaded"
            || !asset.digest.as_deref().is_some_and(valid_sha256_digest)
        {
            return Err(
                "GitHub returned an incomplete or unverifiable release asset; it was rejected."
                    .to_owned(),
            );
        }
    }
    if expected.iter().any(|name| !actual.contains(name.as_str())) {
        return Err(
            "GitHub returned an incomplete or unexpected release asset set; it was rejected."
                .to_owned(),
        );
    }
    Ok(())
}

fn valid_sha256_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|hex| {
        hex.len() == 64
            && hex
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    })
}

fn now_iso() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_else(|_| "1970-01-01T00:00:00Z".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(tag: &str, url: &str) -> Vec<u8> {
        let version = tag.strip_prefix('v').unwrap_or(tag);
        let asset_names = [
            format!("local-browser-bridge-extension-v{version}.zip"),
            format!("local-browser-bridge-v{version}-macos-universal.tar.gz"),
            format!("local-browser-bridge-v{version}-windows-x86_64.exe"),
            format!("local-computer-helper-v{version}-windows-x86_64.exe"),
            "SHA256SUMS.txt".to_owned(),
        ];
        serde_json::to_vec(&serde_json::json!({
            "tag_name": tag,
            "html_url": url,
            "draft": false,
            "prerelease": false,
            "immutable": true,
            "assets": asset_names.map(|name| serde_json::json!({
                "name": name,
                "size": 1,
                "state": "uploaded",
                "digest": format!("sha256:{}", "a".repeat(64))
            }))
        }))
        .unwrap()
    }

    #[test]
    fn identifies_newer_release_as_available() {
        let newer = parse_release(
            &release(
                "v0.6.0",
                "https://github.com/flrngel/local-browser-bridge/releases/tag/v0.6.0",
            ),
            "0.5.0",
        )
        .unwrap();
        assert_eq!(newer.status, UpdateState::Available);
        assert_eq!(newer.latest_version.as_deref(), Some("0.6.0"));
    }

    #[test]
    fn identifies_equal_release_as_up_to_date() {
        let current = parse_release(
            &release(
                "v0.5.0",
                "https://github.com/flrngel/local-browser-bridge/releases/tag/v0.5.0",
            ),
            "0.5.0",
        )
        .unwrap();
        assert_eq!(current.status, UpdateState::UpToDate);
    }

    #[test]
    fn identifies_current_ahead_of_release_as_development() {
        let development = parse_release(
            &release(
                "v0.11.1",
                "https://github.com/flrngel/local-browser-bridge/releases/tag/v0.11.1",
            ),
            "0.12.18",
        )
        .unwrap();

        assert_eq!(development.status, UpdateState::Development);
        assert_eq!(development.latest_version.as_deref(), Some("0.11.1"));
        assert_eq!(
            development.release_url.as_deref(),
            Some("https://github.com/flrngel/local-browser-bridge/releases/tag/v0.11.1")
        );
        assert!(development.message.contains("development build"));
        assert!(development.message.contains("latest public stable release"));
        assert!(
            development
                .message
                .contains("https://github.com/flrngel/local-browser-bridge/releases/tag/v0.11.1")
        );
    }

    #[test]
    fn serializes_update_states_as_stable_wire_values() {
        let cases = [
            (UpdateState::Checking, "checking"),
            (UpdateState::UpToDate, "up_to_date"),
            (UpdateState::Development, "development"),
            (UpdateState::Available, "available"),
            (UpdateState::Error, "error"),
            (UpdateState::Disabled, "disabled"),
        ];

        for (state, expected) in cases {
            assert_eq!(serde_json::to_value(state).unwrap(), expected);
        }
    }

    #[test]
    fn rejects_non_github_links_and_invalid_versions() {
        assert!(parse_release(&release("v0.6.0", "https://evil.example/update"), "0.5.0").is_err());
        assert!(
            parse_release(
                &release(
                    "latest",
                    "https://github.com/flrngel/local-browser-bridge/releases/tag/latest",
                ),
                "0.5.0",
            )
            .is_err()
        );
        assert!(
            parse_release(
                &release(
                    "0.6.0",
                    "https://github.com/flrngel/local-browser-bridge/releases/tag/0.6.0",
                ),
                "0.5.0",
            )
            .is_err()
        );
        assert!(
            parse_release(
                &release(
                    "v0.6.0",
                    "https://github.com/flrngel/local-browser-bridge/releases/tag/v0.7.0",
                ),
                "0.5.0",
            )
            .is_err()
        );
    }

    #[test]
    fn rejects_mutable_release_metadata() {
        let mut value: serde_json::Value = serde_json::from_slice(&release(
            "v0.6.0",
            "https://github.com/flrngel/local-browser-bridge/releases/tag/v0.6.0",
        ))
        .unwrap();
        value["immutable"] = false.into();
        let error = parse_release(&serde_json::to_vec(&value).unwrap(), "0.5.0").unwrap_err();
        assert!(error.contains("mutable release"));
    }

    #[test]
    fn rejects_prerelease_or_build_metadata_on_the_stable_channel() {
        for version in ["0.6.0-rc.1", "0.6.0+rebuilt"] {
            let bytes = release(
                &format!("v{version}"),
                &format!("https://github.com/flrngel/local-browser-bridge/releases/tag/v{version}"),
            );
            let error = parse_release(&bytes, "0.5.0").unwrap_err();
            assert!(error.contains("non-canonical stable release tag"));
        }
    }

    #[test]
    fn rejects_incomplete_or_unverifiable_release_assets() {
        let bytes = release(
            "v0.6.0",
            "https://github.com/flrngel/local-browser-bridge/releases/tag/v0.6.0",
        );
        let baseline: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        let mut cases = Vec::new();
        let mut missing = baseline.clone();
        missing["assets"].as_array_mut().unwrap().pop();
        cases.push(missing);

        let mut extra = baseline.clone();
        extra["assets"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "name": "unexpected.bin",
                "size": 1,
                "state": "uploaded",
                "digest": format!("sha256:{}", "b".repeat(64))
            }));
        cases.push(extra);

        for (field, value) in [
            ("size", serde_json::json!(0)),
            ("state", serde_json::json!("new")),
            ("digest", serde_json::json!("sha256:not-a-digest")),
        ] {
            let mut invalid = baseline.clone();
            invalid["assets"][0][field] = value;
            cases.push(invalid);
        }

        for value in cases {
            assert!(
                parse_release(&serde_json::to_vec(&value).unwrap(), "0.5.0")
                    .unwrap_err()
                    .contains("release asset")
            );
        }
    }

    #[tokio::test]
    async fn checks_bounded_release_metadata_over_http() {
        use axum::Router;
        use axum::routing::get;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let assets = [
            "local-browser-bridge-extension-v0.6.0.zip",
            "local-browser-bridge-v0.6.0-macos-universal.tar.gz",
            "local-browser-bridge-v0.6.0-windows-x86_64.exe",
            "local-computer-helper-v0.6.0-windows-x86_64.exe",
            "SHA256SUMS.txt",
        ]
        .map(|name| {
            serde_json::json!({
                "name": name,
                "size": 1,
                "state": "uploaded",
                "digest": format!("sha256:{}", "a".repeat(64))
            })
        });
        let app = Router::new().route(
            "/latest",
            get(move || async move {
                axum::Json(serde_json::json!({
                    "tag_name": "v0.6.0",
                    "html_url": "https://github.com/flrngel/local-browser-bridge/releases/tag/v0.6.0",
                    "draft": false,
                    "prerelease": false,
                    "immutable": true,
                    "assets": assets
                }))
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let status = check_for_update_at(&format!("http://{address}/latest"), "0.5.0").await;
        assert_eq!(status.status, UpdateState::Available);
        assert_eq!(status.latest_version.as_deref(), Some("0.6.0"));
        server.abort();
    }

    #[tokio::test]
    async fn rejects_oversized_chunked_metadata_without_waiting_for_the_body_to_finish() {
        use tokio::io::AsyncWriteExt as _;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            socket
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nTransfer-Encoding: chunked\r\nConnection: keep-alive\r\n\r\n",
                )
                .await
                .unwrap();
            let chunk = vec![b'a'; MAX_RESPONSE_BYTES / 2 + 1];
            for _ in 0..2 {
                if socket
                    .write_all(format!("{:x}\r\n", chunk.len()).as_bytes())
                    .await
                    .is_err()
                {
                    return;
                }
                if socket.write_all(&chunk).await.is_err()
                    || socket.write_all(b"\r\n").await.is_err()
                {
                    return;
                }
            }
            std::future::pending::<()>().await;
        });

        let status = tokio::time::timeout(
            Duration::from_secs(2),
            check_for_update_at(&format!("http://{address}/latest"), "0.5.0"),
        )
        .await
        .expect("the checker must reject before the unfinished body times out");
        assert_eq!(status.status, UpdateState::Error);
        assert!(status.message.contains("oversized release metadata"));
        server.abort();
    }
}

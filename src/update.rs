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

    let response = match client
        .get(api_url)
        .header("Accept", "application/vnd.github+json")
        .header("X-GitHub-Api-Version", "2022-11-28")
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

    let bytes = match response.bytes().await {
        Ok(bytes) if bytes.len() <= MAX_RESPONSE_BYTES => bytes,
        Ok(_) => {
            return UpdateStatus::failed(
                "GitHub returned oversized release metadata; it was rejected.",
            );
        }
        Err(_) => {
            return UpdateStatus::failed(
                "GitHub release metadata could not be read. No files were downloaded.",
            );
        }
    };
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
    if !release.html_url.starts_with(RELEASE_TAG_PREFIX)
        || release.html_url.len() > 2_048
        || release.html_url.chars().any(char::is_whitespace)
    {
        return Err("GitHub returned an unexpected release link; it was rejected.".to_owned());
    }

    let latest_text = release
        .tag_name
        .strip_prefix('v')
        .unwrap_or(&release.tag_name);
    let current = Version::parse(current_version)
        .map_err(|_| "The installed version is not valid semantic versioning.".to_owned())?;
    let latest = Version::parse(latest_text)
        .map_err(|_| "GitHub returned an invalid release version; it was rejected.".to_owned())?;
    let available = latest > current;
    Ok(UpdateStatus {
        status: if available {
            UpdateState::Available
        } else {
            UpdateState::UpToDate
        },
        current_version: current.to_string(),
        latest_version: Some(latest.to_string()),
        release_url: Some(release.html_url),
        checked_at: Some(now_iso()),
        message: if available {
            format!(
                "Version {latest} is available. Review and download it from the official GitHub release."
            )
        } else {
            format!("Version {current} is the latest stable release.")
        },
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
        serde_json::to_vec(&serde_json::json!({
            "tag_name": tag,
            "html_url": url,
            "draft": false,
            "prerelease": false
        }))
        .unwrap()
    }

    #[test]
    fn identifies_newer_and_current_releases() {
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
    }

    #[tokio::test]
    async fn checks_bounded_release_metadata_over_http() {
        use axum::Router;
        use axum::routing::get;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind(("127.0.0.1", 0)).await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().route(
            "/latest",
            get(|| async {
                axum::Json(serde_json::json!({
                    "tag_name": "v0.6.0",
                    "html_url": "https://github.com/flrngel/local-browser-bridge/releases/tag/v0.6.0",
                    "draft": false,
                    "prerelease": false
                }))
            }),
        );
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

        let status = check_for_update_at(&format!("http://{address}/latest"), "0.5.0").await;
        assert_eq!(status.status, UpdateState::Available);
        assert_eq!(status.latest_version.as_deref(), Some("0.6.0"));
        server.abort();
    }
}

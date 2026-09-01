//! Checks GitHub Releases for a newer FPV Editor build (PLAN.md settings:
//! "check for updates"). No signing/auto-replace infrastructure exists yet,
//! so an available update is surfaced as a downloadable platform installer
//! (dmg/msi/AppImage/deb) that the user runs themselves — see
//! `AppState::download_update`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const REPO_OWNER: &str = "mroxso";
const REPO_NAME: &str = "FPVEditor";
const USER_AGENT: &str = "fpv-editor-update-checker";
const GITHUB_API_BASE: &str = "https://api.github.com";

#[derive(Debug, Clone, Serialize)]
pub struct UpdateCheckResult {
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
    pub release_url: String,
    pub release_notes: Option<String>,
    pub published_at: Option<String>,
    pub download_url: Option<String>,
    pub asset_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct ReleaseAsset {
    name: String,
    browser_download_url: String,
}

#[derive(Debug, Deserialize)]
struct GithubRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    published_at: Option<String>,
    #[serde(default)]
    assets: Vec<ReleaseAsset>,
}

pub async fn check_for_updates(current_version: &str) -> Result<UpdateCheckResult> {
    check_for_updates_against(GITHUB_API_BASE, current_version).await
}

async fn check_for_updates_against(
    api_base: &str,
    current_version: &str,
) -> Result<UpdateCheckResult> {
    let url = format!("{api_base}/repos/{REPO_OWNER}/{REPO_NAME}/releases/latest");
    let client = reqwest::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .context("failed to reach GitHub releases")?;

    if !response.status().is_success() {
        anyhow::bail!(
            "GitHub returned {} while checking for updates",
            response.status()
        );
    }

    let release: GithubRelease = response
        .json()
        .await
        .context("failed to parse GitHub release response")?;

    let latest_version = release.tag_name.trim_start_matches('v').to_string();
    let update_available = is_newer(&latest_version, current_version.trim_start_matches('v'));
    let asset = pick_asset_for_platform(&release.assets);

    Ok(UpdateCheckResult {
        current_version: current_version.to_string(),
        latest_version,
        update_available,
        release_url: release.html_url,
        release_notes: release.body,
        published_at: release.published_at,
        download_url: asset
            .as_ref()
            .map(|asset| asset.browser_download_url.clone()),
        asset_name: asset.map(|asset| asset.name),
    })
}

/// Download the update asset to a temp file and return its path so the
/// caller (the Tauri command, which has an `AppHandle`) can hand it to the
/// OS's default opener — this crate stays windowing-runtime-free.
pub async fn download_update(download_url: &str, asset_name: &str) -> Result<PathBuf> {
    let client = reqwest::Client::new();
    let response = client
        .get(download_url)
        .header("User-Agent", USER_AGENT)
        .send()
        .await
        .context("failed to download update")?;

    if !response.status().is_success() {
        anyhow::bail!("update download failed with status {}", response.status());
    }

    let bytes = response
        .bytes()
        .await
        .context("failed to read update download")?;
    let dest = std::env::temp_dir().join(asset_name);
    tokio::fs::write(&dest, &bytes)
        .await
        .context("failed to save downloaded update")?;
    Ok(dest)
}

fn pick_asset_for_platform(assets: &[ReleaseAsset]) -> Option<ReleaseAsset> {
    let preferred_suffixes: &[&str] = if cfg!(target_os = "macos") {
        &[".dmg"]
    } else if cfg!(target_os = "windows") {
        &[".msi"]
    } else {
        &[".AppImage", ".deb"]
    };
    preferred_suffixes.iter().find_map(|suffix| {
        assets
            .iter()
            .find(|asset| asset.name.ends_with(suffix))
            .cloned()
    })
}

/// Loose dotted-version comparison (no pre-release/build metadata handling):
/// good enough for comparing this project's plain `MAJOR.MINOR.PATCH` tags
/// without pulling in a full semver dependency.
fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split(['.', '-', '+'])
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let latest = parse(latest);
    let current = parse(current);
    for i in 0..latest.len().max(current.len()) {
        let l = latest.get(i).copied().unwrap_or(0);
        let c = current.get(i).copied().unwrap_or(0);
        if l != c {
            return l > c;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[test]
    fn newer_patch_and_minor_versions_are_detected() {
        assert!(is_newer("0.3.1", "0.3.0"));
        assert!(is_newer("0.4.0", "0.3.9"));
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.3.0", "0.3.0"));
        assert!(!is_newer("0.2.9", "0.3.0"));
    }

    #[tokio::test]
    async fn reports_update_available_with_a_matching_platform_asset() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/mroxso/FPVEditor/releases/latest"))
            .and(header("User-Agent", USER_AGENT))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tag_name": "v9.9.9",
                "html_url": "https://github.com/mroxso/FPVEditor/releases/tag/v9.9.9",
                "body": "Release notes",
                "published_at": "2026-01-01T00:00:00Z",
                "assets": [
                    { "name": "FPVEditor_9.9.9_aarch64.dmg", "browser_download_url": "https://example.com/app.dmg" },
                    { "name": "FPVEditor_9.9.9_x64.msi", "browser_download_url": "https://example.com/app.msi" },
                    { "name": "fpv-editor_9.9.9_amd64.AppImage", "browser_download_url": "https://example.com/app.AppImage" }
                ]
            })))
            .mount(&server)
            .await;

        let result = check_for_updates_against(&server.uri(), "0.3.0")
            .await
            .unwrap();

        assert_eq!(result.latest_version, "9.9.9");
        assert!(result.update_available);
        assert!(result.download_url.is_some());
        assert_eq!(result.release_notes.as_deref(), Some("Release notes"));
    }

    #[tokio::test]
    async fn reports_no_update_when_already_current() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/mroxso/FPVEditor/releases/latest"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "tag_name": "v0.3.0",
                "html_url": "https://github.com/mroxso/FPVEditor/releases/tag/v0.3.0",
                "body": null,
                "published_at": null,
                "assets": []
            })))
            .mount(&server)
            .await;

        let result = check_for_updates_against(&server.uri(), "0.3.0")
            .await
            .unwrap();

        assert!(!result.update_available);
        assert!(result.download_url.is_none());
    }

    #[tokio::test]
    async fn surfaces_a_clear_error_when_github_is_unreachable_or_erroring() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/repos/mroxso/FPVEditor/releases/latest"))
            .respond_with(ResponseTemplate::new(503))
            .mount(&server)
            .await;

        let error = check_for_updates_against(&server.uri(), "0.3.0")
            .await
            .unwrap_err();
        assert!(error.to_string().contains("503"));
    }
}

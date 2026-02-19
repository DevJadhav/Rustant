//! Self-update mechanism for Rustant.
//!
//! Checks for new versions via the GitHub Releases API and can download
//! and replace the running binary.

use serde::{Deserialize, Serialize};

/// Current version of Rustant.
pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// GitHub repository for release checks.
const GITHUB_OWNER: &str = "DevJadhav";
const GITHUB_REPO: &str = "Rustant";

/// Configuration for the update system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    /// Whether to automatically check for updates.
    #[serde(default = "default_true")]
    pub auto_check: bool,
    /// Hours between update checks.
    #[serde(default = "default_check_interval")]
    pub check_interval_hours: u64,
    /// Release channel: "stable" or "beta".
    #[serde(default = "default_channel")]
    pub channel: String,
}

fn default_true() -> bool {
    true
}

fn default_check_interval() -> u64 {
    24
}

fn default_channel() -> String {
    "stable".into()
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            auto_check: true,
            check_interval_hours: 24,
            channel: "stable".into(),
        }
    }
}

/// Result of an update check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateCheckResult {
    /// Current installed version.
    pub current_version: String,
    /// Latest available version (if found).
    pub latest_version: Option<String>,
    /// Whether an update is available.
    pub update_available: bool,
    /// Release URL (if available).
    pub release_url: Option<String>,
    /// Release notes (if available).
    pub release_notes: Option<String>,
}

/// Check for available updates by querying GitHub Releases.
pub struct UpdateChecker {
    config: UpdateConfig,
}

impl UpdateChecker {
    /// Create a new update checker.
    pub fn new(config: UpdateConfig) -> Self {
        Self { config }
    }

    /// Check if an update is available.
    pub async fn check(&self) -> Result<UpdateCheckResult, UpdateError> {
        let client = reqwest::Client::builder()
            .user_agent(format!("rustant/{CURRENT_VERSION}"))
            .build()
            .map_err(|e| UpdateError::NetworkError(e.to_string()))?;

        let url =
            format!("https://api.github.com/repos/{GITHUB_OWNER}/{GITHUB_REPO}/releases/latest");

        let response = client
            .get(&url)
            .send()
            .await
            .map_err(|e| UpdateError::NetworkError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(UpdateError::NetworkError(format!(
                "GitHub API returned status {}",
                response.status()
            )));
        }

        let release: GitHubRelease = response
            .json()
            .await
            .map_err(|e| UpdateError::ParseError(e.to_string()))?;

        let latest_version = release.tag_name.trim_start_matches('v').to_string();
        let update_available = is_newer_version(&latest_version, CURRENT_VERSION);

        // Filter by channel
        if self.config.channel == "stable" && release.prerelease {
            return Ok(UpdateCheckResult {
                current_version: CURRENT_VERSION.into(),
                latest_version: Some(latest_version),
                update_available: false,
                release_url: Some(release.html_url),
                release_notes: Some(release.body.unwrap_or_default()),
            });
        }

        Ok(UpdateCheckResult {
            current_version: CURRENT_VERSION.into(),
            latest_version: Some(latest_version),
            update_available,
            release_url: Some(release.html_url),
            release_notes: Some(release.body.unwrap_or_default()),
        })
    }

    /// Get the update configuration.
    pub fn config(&self) -> &UpdateConfig {
        &self.config
    }
}

/// Performs the actual binary update.
pub struct Updater;

impl Updater {
    /// Download and install the latest version.
    pub fn update() -> Result<(), UpdateError> {
        let status = self_update::backends::github::Update::configure()
            .repo_owner(GITHUB_OWNER)
            .repo_name(GITHUB_REPO)
            .bin_name("rustant")
            .current_version(CURRENT_VERSION)
            .show_output(true)
            .show_download_progress(true)
            .build()
            .map_err(|e| UpdateError::UpdateFailed(e.to_string()))?
            .update()
            .map_err(|e| UpdateError::UpdateFailed(e.to_string()))?;

        tracing::info!(
            old_version = CURRENT_VERSION,
            new_version = %status.version(),
            "Updated successfully"
        );

        Ok(())
    }
}

/// Errors from update operations.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Parse error: {0}")]
    ParseError(String),
    #[error("Update failed: {0}")]
    UpdateFailed(String),
}

/// GitHub release API response (subset).
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    html_url: String,
    body: Option<String>,
    prerelease: bool,
}

/// Compare two semver versions. Returns true if `latest` is newer than `current`.
pub fn is_newer_version(latest: &str, current: &str) -> bool {
    let latest_parts: Vec<u32> = latest.split('.').filter_map(|p| p.parse().ok()).collect();
    let current_parts: Vec<u32> = current.split('.').filter_map(|p| p.parse().ok()).collect();

    for i in 0..3 {
        let l = latest_parts.get(i).copied().unwrap_or(0);
        let c = current_parts.get(i).copied().unwrap_or(0);
        if l > c {
            return true;
        }
        if l < c {
            return false;
        }
    }
    false // Equal
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_newer_version() {
        assert!(is_newer_version("1.1.0", "1.0.0"));
        assert!(is_newer_version("2.0.0", "1.9.9"));
        assert!(is_newer_version("0.2.0", "0.1.0"));
        assert!(is_newer_version("0.1.1", "0.1.0"));
    }

    #[test]
    fn test_is_not_newer_version() {
        assert!(!is_newer_version("1.0.0", "1.0.0"));
        assert!(!is_newer_version("0.9.0", "1.0.0"));
        assert!(!is_newer_version("0.1.0", "0.2.0"));
    }

    #[test]
    fn test_update_config_defaults() {
        let config = UpdateConfig::default();
        assert!(config.auto_check);
        assert_eq!(config.check_interval_hours, 24);
        assert_eq!(config.channel, "stable");
    }

    #[test]
    fn test_update_config_serialization() {
        let config = UpdateConfig {
            auto_check: false,
            check_interval_hours: 12,
            channel: "beta".into(),
        };
        let json = serde_json::to_string(&config).unwrap();
        let restored: UpdateConfig = serde_json::from_str(&json).unwrap();
        assert!(!restored.auto_check);
        assert_eq!(restored.check_interval_hours, 12);
        assert_eq!(restored.channel, "beta");
    }

    #[test]
    fn test_update_check_result_serialization() {
        let result = UpdateCheckResult {
            current_version: "0.1.0".into(),
            latest_version: Some("0.2.0".into()),
            update_available: true,
            release_url: Some("https://github.com/DevJadhav/Rustant/releases/v0.2.0".into()),
            release_notes: Some("New features".into()),
        };
        let json = serde_json::to_string(&result).unwrap();
        let restored: UpdateCheckResult = serde_json::from_str(&json).unwrap();
        assert!(restored.update_available);
        assert_eq!(restored.latest_version, Some("0.2.0".into()));
    }

    #[test]
    #[allow(clippy::const_is_empty)]
    fn test_current_version_defined() {
        assert!(!CURRENT_VERSION.is_empty());
    }

    #[test]
    fn test_update_checker_creation() {
        let config = UpdateConfig::default();
        let checker = UpdateChecker::new(config);
        assert!(checker.config().auto_check);
    }

    #[test]
    fn test_version_comparison_edge_cases() {
        assert!(!is_newer_version("0.0.0", "0.0.0"));
        assert!(is_newer_version("0.0.1", "0.0.0"));
        assert!(is_newer_version("10.0.0", "9.9.9"));
    }
}

//! Browser security guard — URL allowlist/blocklist filtering and credential masking.

use serde::{Deserialize, Serialize};
use url::Url;

/// Security guard that enforces URL restrictions and masks sensitive content.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BrowserSecurityGuard {
    /// If non-empty, only these domains are allowed.
    pub allowed_domains: Vec<String>,
    /// These domains are always blocked.
    pub blocked_domains: Vec<String>,
}

impl BrowserSecurityGuard {
    /// Create a new security guard with the given allowlist and blocklist.
    pub fn new(allowed_domains: Vec<String>, blocked_domains: Vec<String>) -> Self {
        Self {
            allowed_domains,
            blocked_domains,
        }
    }

    /// Check whether a URL is allowed by the security policy.
    ///
    /// Rules:
    /// 1. If blocked_domains is non-empty and the URL's host matches, it is blocked.
    /// 2. If allowed_domains is non-empty and the URL's host does NOT match, it is blocked.
    /// 3. Otherwise, the URL is allowed.
    pub fn check_url(&self, url_str: &str) -> Result<(), String> {
        let url = Url::parse(url_str).map_err(|e| format!("Invalid URL: {e}"))?;
        let host = url.host_str().unwrap_or("");

        // Check blocklist first
        if self
            .blocked_domains
            .iter()
            .any(|d| host == d.as_str() || host.ends_with(&format!(".{d}")))
        {
            return Err(format!("URL blocked by security policy: {url_str}"));
        }

        // Check allowlist (if non-empty, URL must match)
        if !self.allowed_domains.is_empty()
            && !self
                .allowed_domains
                .iter()
                .any(|d| host == d.as_str() || host.ends_with(&format!(".{d}")))
        {
            return Err(format!("URL not in allowlist: {url_str}"));
        }

        Ok(())
    }

    /// Mask credentials and sensitive content in a string.
    ///
    /// Replaces patterns like `password=...`, `token=...`, `secret=...`,
    /// `Authorization: Bearer ...` with masked versions.
    pub fn mask_credentials(content: &str) -> String {
        let mut result = content.to_string();

        // Mask common credential patterns in query strings / form data
        let sensitive_keys = [
            "password",
            "passwd",
            "pwd",
            "token",
            "secret",
            "api_key",
            "apikey",
            "access_token",
            "refresh_token",
            "auth",
        ];

        for key in &sensitive_keys {
            // Match key=value patterns (url-encoded or plain)
            let patterns = [format!("{key}="), format!("{key}%3D"), format!("{key}%3d")];
            for pattern in &patterns {
                if let Some(start) = result.to_lowercase().find(&pattern.to_lowercase()) {
                    let value_start = start + pattern.len();
                    // Find end of value (space, &, or end of string)
                    let value_end = result[value_start..]
                        .find(['&', ' ', '\n', '"'])
                        .map(|i| value_start + i)
                        .unwrap_or(result.len());
                    if value_end > value_start {
                        result.replace_range(value_start..value_end, "***MASKED***");
                    }
                }
            }
        }

        // Mask Authorization headers
        if let Some(start) = result.find("Authorization:") {
            let header_start = start + "Authorization:".len();
            let header_end = result[header_start..]
                .find('\n')
                .map(|i| header_start + i)
                .unwrap_or(result.len());
            result.replace_range(header_start..header_end, " ***MASKED***");
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_url_allowed_when_no_lists() {
        let guard = BrowserSecurityGuard::default();
        assert!(guard.check_url("https://example.com").is_ok());
        assert!(guard.check_url("https://any-site.org/path?q=1").is_ok());
    }

    #[test]
    fn test_check_url_blocked_by_blocklist() {
        let guard = BrowserSecurityGuard::new(vec![], vec!["evil.com".to_string()]);
        assert!(guard.check_url("https://evil.com/malware").is_err());
        assert!(guard.check_url("https://sub.evil.com").is_err());
        // Other domains should be allowed
        assert!(guard.check_url("https://example.com").is_ok());
    }

    #[test]
    fn test_check_url_allowed_by_allowlist() {
        let guard = BrowserSecurityGuard::new(
            vec!["example.com".to_string(), "docs.rs".to_string()],
            vec![],
        );
        assert!(guard.check_url("https://example.com/page").is_ok());
        assert!(guard.check_url("https://docs.rs/tokio").is_ok());
        assert!(guard.check_url("https://sub.example.com").is_ok());
    }

    #[test]
    fn test_check_url_not_in_allowlist_rejected() {
        let guard = BrowserSecurityGuard::new(vec!["example.com".to_string()], vec![]);
        assert!(guard.check_url("https://other-site.com").is_err());
        let err = guard.check_url("https://other-site.com").unwrap_err();
        assert!(err.contains("not in allowlist"));
    }

    #[test]
    fn test_mask_credentials_hides_passwords() {
        let input = "login: user password=SuperSecret123&next=/dashboard";
        let masked = BrowserSecurityGuard::mask_credentials(input);
        assert!(!masked.contains("SuperSecret123"));
        assert!(masked.contains("***MASKED***"));
        assert!(masked.contains("login: user"));
    }

    #[test]
    fn test_mask_credentials_preserves_other_content() {
        let input = "Hello world, no secrets here.";
        let masked = BrowserSecurityGuard::mask_credentials(input);
        assert_eq!(masked, input);
    }

    #[test]
    fn test_mask_credentials_hides_authorization_header() {
        let input = "Authorization: Bearer eyJtoken123\nContent-Type: text/html";
        let masked = BrowserSecurityGuard::mask_credentials(input);
        assert!(!masked.contains("eyJtoken123"));
        assert!(masked.contains("***MASKED***"));
        assert!(masked.contains("Content-Type: text/html"));
    }

    #[test]
    fn test_mask_credentials_hides_api_keys() {
        let input = "api_key=sk-12345abc&format=json";
        let masked = BrowserSecurityGuard::mask_credentials(input);
        assert!(!masked.contains("sk-12345abc"));
        assert!(masked.contains("***MASKED***"));
        assert!(masked.contains("format=json"));
    }

    #[test]
    fn test_check_url_invalid_url() {
        let guard = BrowserSecurityGuard::default();
        assert!(guard.check_url("not a valid url").is_err());
    }

    #[test]
    fn test_blocklist_takes_priority_over_allowlist() {
        let guard =
            BrowserSecurityGuard::new(vec!["evil.com".to_string()], vec!["evil.com".to_string()]);
        // Blocklist should win even if domain is in allowlist
        assert!(guard.check_url("https://evil.com").is_err());
        let err = guard.check_url("https://evil.com").unwrap_err();
        assert!(err.contains("blocked"));
    }
}

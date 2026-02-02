//! Browser session management — lifecycle, page pool, and cleanup.

use crate::browser::cdp::CdpClient;
use crate::browser::security::BrowserSecurityGuard;
use crate::browser::snapshot::{PageSnapshot, SnapshotMode};
use crate::error::BrowserError;
use std::sync::Arc;

/// Manages a browser session including page lifecycle and security enforcement.
pub struct BrowserSession {
    /// The CDP client used for browser interaction.
    client: Arc<dyn CdpClient>,
    /// Security guard for URL filtering and credential masking.
    security: Arc<BrowserSecurityGuard>,
    /// Maximum number of pages/tabs allowed.
    max_pages: usize,
    /// Current number of open pages.
    open_pages: usize,
    /// Whether the session is active.
    active: bool,
}

impl BrowserSession {
    /// Create a new browser session.
    pub fn new(
        client: Arc<dyn CdpClient>,
        security: Arc<BrowserSecurityGuard>,
        max_pages: usize,
    ) -> Self {
        Self {
            client,
            security,
            max_pages,
            open_pages: 1, // Start with one page
            active: true,
        }
    }

    /// Get the underlying CDP client.
    pub fn client(&self) -> &Arc<dyn CdpClient> {
        &self.client
    }

    /// Get the security guard.
    pub fn security(&self) -> &Arc<BrowserSecurityGuard> {
        &self.security
    }

    /// Navigate to a URL, checking security restrictions first.
    pub async fn navigate(&self, url: &str) -> Result<(), BrowserError> {
        if !self.active {
            return Err(BrowserError::SessionError {
                message: "Session is closed".to_string(),
            });
        }
        self.security
            .check_url(url)
            .map_err(|msg| BrowserError::UrlBlocked { url: msg })?;
        self.client.navigate(url).await
    }

    /// Take a snapshot of the current page in the specified mode.
    pub async fn snapshot(&self, mode: SnapshotMode) -> Result<PageSnapshot, BrowserError> {
        if !self.active {
            return Err(BrowserError::SessionError {
                message: "Session is closed".to_string(),
            });
        }

        let url = self.client.get_url().await?;
        let title = self.client.get_title().await?;

        let content = match &mode {
            SnapshotMode::Html => self.client.get_html().await?,
            SnapshotMode::Text => self.client.get_text().await?,
            SnapshotMode::AriaTree => self.client.get_aria_tree().await?,
            SnapshotMode::Screenshot => {
                let bytes = self.client.screenshot().await?;
                base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes)
            }
        };

        // Mask credentials in non-screenshot content
        let content = if mode != SnapshotMode::Screenshot {
            BrowserSecurityGuard::mask_credentials(&content)
        } else {
            content
        };

        Ok(PageSnapshot::new(url, title, mode, content))
    }

    /// Check if a new page can be opened.
    pub fn can_open_page(&self) -> bool {
        self.open_pages < self.max_pages
    }

    /// Record a new page being opened.
    pub fn open_page(&mut self) -> Result<(), BrowserError> {
        if self.open_pages >= self.max_pages {
            return Err(BrowserError::PageLimitExceeded {
                max: self.max_pages,
            });
        }
        self.open_pages += 1;
        Ok(())
    }

    /// Record a page being closed.
    pub fn close_page(&mut self) {
        if self.open_pages > 0 {
            self.open_pages -= 1;
        }
    }

    /// Close the entire browser session.
    pub async fn close(&mut self) -> Result<(), BrowserError> {
        if self.active {
            self.active = false;
            self.client.close().await?;
        }
        Ok(())
    }

    /// Whether the session is still active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Current number of open pages.
    pub fn open_page_count(&self) -> usize {
        self.open_pages
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::cdp::MockCdpClient;

    fn make_session(max_pages: usize) -> (BrowserSession, Arc<MockCdpClient>) {
        let client = Arc::new(MockCdpClient::new());
        let security = Arc::new(BrowserSecurityGuard::default());
        let session = BrowserSession::new(client.clone(), security, max_pages);
        (session, client)
    }

    fn make_session_with_security(
        security: BrowserSecurityGuard,
    ) -> (BrowserSession, Arc<MockCdpClient>) {
        let client = Arc::new(MockCdpClient::new());
        let security = Arc::new(security);
        let session = BrowserSession::new(client.clone(), security, 5);
        (session, client)
    }

    #[tokio::test]
    async fn test_session_navigate() {
        let (session, client) = make_session(5);
        session.navigate("https://example.com").await.unwrap();
        assert_eq!(*client.current_url.lock().unwrap(), "https://example.com");
    }

    #[tokio::test]
    async fn test_session_navigate_blocked_url() {
        let security = BrowserSecurityGuard::new(vec![], vec!["evil.com".to_string()]);
        let (session, _client) = make_session_with_security(security);
        let result = session.navigate("https://evil.com").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_snapshot_html() {
        let (session, client) = make_session(5);
        client.set_url("https://example.com");
        client.set_title("Example");
        client.set_html("<html><body>Hello</body></html>");
        let snap = session.snapshot(SnapshotMode::Html).await.unwrap();
        assert_eq!(snap.url, "https://example.com");
        assert_eq!(snap.title, "Example");
        assert_eq!(snap.mode, SnapshotMode::Html);
        assert!(snap.content.contains("Hello"));
    }

    #[tokio::test]
    async fn test_session_snapshot_text() {
        let (session, client) = make_session(5);
        client.set_url("https://docs.rs");
        client.set_title("Docs");
        client.set_text("Welcome to docs.rs");
        let snap = session.snapshot(SnapshotMode::Text).await.unwrap();
        assert_eq!(snap.mode, SnapshotMode::Text);
        assert_eq!(snap.content, "Welcome to docs.rs");
    }

    #[tokio::test]
    async fn test_session_snapshot_aria_tree() {
        let (session, client) = make_session(5);
        client.set_url("https://example.com");
        client.set_title("Example");
        client.set_aria_tree("document\n  heading 'Title'\n  button 'Submit'");
        let snap = session.snapshot(SnapshotMode::AriaTree).await.unwrap();
        assert_eq!(snap.mode, SnapshotMode::AriaTree);
        assert!(snap.content.contains("heading"));
    }

    #[tokio::test]
    async fn test_session_snapshot_screenshot() {
        let (session, client) = make_session(5);
        client.set_url("https://example.com");
        client.set_title("Example");
        client.set_screenshot(vec![1, 2, 3, 4, 5]);
        let snap = session.snapshot(SnapshotMode::Screenshot).await.unwrap();
        assert_eq!(snap.mode, SnapshotMode::Screenshot);
        // Content should be base64-encoded
        assert!(!snap.content.is_empty());
    }

    #[tokio::test]
    async fn test_session_page_limit() {
        let (mut session, _client) = make_session(2);
        // Session starts with 1 page
        assert_eq!(session.open_page_count(), 1);
        assert!(session.can_open_page());
        session.open_page().unwrap();
        assert_eq!(session.open_page_count(), 2);
        assert!(!session.can_open_page());
        let result = session.open_page();
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_close_page() {
        let (mut session, _client) = make_session(3);
        session.open_page().unwrap();
        assert_eq!(session.open_page_count(), 2);
        session.close_page();
        assert_eq!(session.open_page_count(), 1);
    }

    #[tokio::test]
    async fn test_session_close() {
        let (mut session, client) = make_session(5);
        assert!(session.is_active());
        session.close().await.unwrap();
        assert!(!session.is_active());
        assert!(*client.closed.lock().unwrap());
    }

    #[tokio::test]
    async fn test_session_navigate_after_close() {
        let (mut session, _client) = make_session(5);
        session.close().await.unwrap();
        let result = session.navigate("https://example.com").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_snapshot_after_close() {
        let (mut session, _client) = make_session(5);
        session.close().await.unwrap();
        let result = session.snapshot(SnapshotMode::Html).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_session_security_guard_gates_navigate() {
        let security = BrowserSecurityGuard::new(vec!["allowed.com".to_string()], vec![]);
        let (session, _client) = make_session_with_security(security);
        // Allowed domain succeeds
        assert!(session.navigate("https://allowed.com/page").await.is_ok());
        // Non-allowed domain fails
        assert!(session.navigate("https://other.com").await.is_err());
    }

    #[tokio::test]
    async fn test_session_double_close_is_safe() {
        let (mut session, _client) = make_session(5);
        session.close().await.unwrap();
        // Second close should be a no-op
        session.close().await.unwrap();
        assert!(!session.is_active());
    }
}

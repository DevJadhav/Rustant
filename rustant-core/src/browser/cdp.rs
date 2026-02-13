//! CDP (Chrome DevTools Protocol) client trait and mock implementation.
//!
//! The `CdpClient` trait abstracts all browser interactions, enabling
//! mock-based testing without a real Chrome instance.

use crate::error::BrowserError;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Mutex;

/// Metadata about an open browser tab/page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TabInfo {
    /// Unique tab identifier (Chrome target ID).
    pub id: String,
    /// Current URL of the tab.
    pub url: String,
    /// Current title of the tab.
    pub title: String,
    /// Whether this is the currently active tab.
    pub active: bool,
}

/// Trait abstracting Chrome DevTools Protocol operations.
///
/// Implementors include `MockCdpClient` (for tests) and
/// a real `ChromiumCdpClient` (wrapping chromiumoxide) for production.
#[async_trait]
pub trait CdpClient: Send + Sync {
    /// Navigate to the given URL.
    async fn navigate(&self, url: &str) -> Result<(), BrowserError>;

    /// Go back in browser history.
    async fn go_back(&self) -> Result<(), BrowserError>;

    /// Go forward in browser history.
    async fn go_forward(&self) -> Result<(), BrowserError>;

    /// Refresh the current page.
    async fn refresh(&self) -> Result<(), BrowserError>;

    /// Click an element matching the CSS selector.
    async fn click(&self, selector: &str) -> Result<(), BrowserError>;

    /// Type text into the currently focused element.
    async fn type_text(&self, selector: &str, text: &str) -> Result<(), BrowserError>;

    /// Clear and fill a form field.
    async fn fill(&self, selector: &str, value: &str) -> Result<(), BrowserError>;

    /// Select an option in a `<select>` element.
    async fn select_option(&self, selector: &str, value: &str) -> Result<(), BrowserError>;

    /// Hover over an element.
    async fn hover(&self, selector: &str) -> Result<(), BrowserError>;

    /// Press a keyboard key.
    async fn press_key(&self, key: &str) -> Result<(), BrowserError>;

    /// Scroll by the given pixel offsets.
    async fn scroll(&self, x: i32, y: i32) -> Result<(), BrowserError>;

    /// Take a screenshot and return PNG bytes.
    async fn screenshot(&self) -> Result<Vec<u8>, BrowserError>;

    /// Get the full page HTML.
    async fn get_html(&self) -> Result<String, BrowserError>;

    /// Get the visible text content.
    async fn get_text(&self) -> Result<String, BrowserError>;

    /// Get the current page URL.
    async fn get_url(&self) -> Result<String, BrowserError>;

    /// Get the current page title.
    async fn get_title(&self) -> Result<String, BrowserError>;

    /// Evaluate a JavaScript expression and return the result.
    async fn evaluate_js(&self, script: &str) -> Result<Value, BrowserError>;

    /// Wait for an element matching the selector to appear.
    async fn wait_for_selector(&self, selector: &str, timeout_ms: u64) -> Result<(), BrowserError>;

    /// Get the accessibility / ARIA tree as a string.
    async fn get_aria_tree(&self) -> Result<String, BrowserError>;

    /// Upload a file to a file input element.
    async fn upload_file(&self, selector: &str, path: &str) -> Result<(), BrowserError>;

    /// Close the current page/tab.
    async fn close(&self) -> Result<(), BrowserError>;

    // --- Tab/page management methods ---

    /// Open a new tab and navigate to the given URL. Returns the tab ID.
    async fn new_tab(&self, url: &str) -> Result<String, BrowserError>;

    /// List all open tabs with their metadata.
    async fn list_tabs(&self) -> Result<Vec<TabInfo>, BrowserError>;

    /// Switch the active tab to the one with the given ID.
    async fn switch_tab(&self, tab_id: &str) -> Result<(), BrowserError>;

    /// Close a specific tab by ID.
    async fn close_tab(&self, tab_id: &str) -> Result<(), BrowserError>;

    /// Get the ID of the currently active tab.
    async fn active_tab_id(&self) -> Result<String, BrowserError>;
}

/// A mock CDP client for testing. Records all calls and returns configurable results.
pub struct MockCdpClient {
    /// Current URL (set by navigate).
    pub current_url: Mutex<String>,
    /// Current page title.
    pub current_title: Mutex<String>,
    /// HTML content to return from get_html().
    pub html_content: Mutex<String>,
    /// Text content to return from get_text().
    pub text_content: Mutex<String>,
    /// ARIA tree to return from get_aria_tree().
    pub aria_tree: Mutex<String>,
    /// Screenshot bytes to return.
    pub screenshot_bytes: Mutex<Vec<u8>>,
    /// JavaScript results keyed by script.
    pub js_results: Mutex<HashMap<String, Value>>,
    /// Record of all method calls for assertion: (method, args).
    pub call_log: Mutex<Vec<(String, Vec<String>)>>,
    /// If set, navigate will return this error.
    pub navigate_error: Mutex<Option<BrowserError>>,
    /// If set, click will return this error.
    pub click_error: Mutex<Option<BrowserError>>,
    /// If set, wait_for_selector will return this error.
    pub wait_error: Mutex<Option<BrowserError>>,
    /// Whether the client is "closed".
    pub closed: Mutex<bool>,
    /// Open tabs for tab management testing.
    pub tabs: Mutex<Vec<TabInfo>>,
    /// ID of the currently active tab.
    pub active_tab: Mutex<String>,
    /// Counter for generating unique tab IDs.
    tab_counter: Mutex<u32>,
}

impl Default for MockCdpClient {
    fn default() -> Self {
        let default_tab_id = "tab-0".to_string();
        Self {
            current_url: Mutex::new("about:blank".to_string()),
            current_title: Mutex::new(String::new()),
            html_content: Mutex::new("<html><body></body></html>".to_string()),
            text_content: Mutex::new(String::new()),
            aria_tree: Mutex::new("document\n  body".to_string()),
            screenshot_bytes: Mutex::new(vec![0x89, 0x50, 0x4E, 0x47]), // PNG magic bytes
            js_results: Mutex::new(HashMap::new()),
            call_log: Mutex::new(Vec::new()),
            navigate_error: Mutex::new(None),
            click_error: Mutex::new(None),
            wait_error: Mutex::new(None),
            closed: Mutex::new(false),
            tabs: Mutex::new(vec![TabInfo {
                id: default_tab_id.clone(),
                url: "about:blank".to_string(),
                title: String::new(),
                active: true,
            }]),
            active_tab: Mutex::new(default_tab_id),
            tab_counter: Mutex::new(1),
        }
    }
}

impl MockCdpClient {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the URL that will be returned by get_url() and navigate().
    pub fn set_url(&self, url: impl Into<String>) {
        *self.current_url.lock().unwrap() = url.into();
    }

    /// Set the title returned by get_title().
    pub fn set_title(&self, title: impl Into<String>) {
        *self.current_title.lock().unwrap() = title.into();
    }

    /// Set the HTML returned by get_html().
    pub fn set_html(&self, html: impl Into<String>) {
        *self.html_content.lock().unwrap() = html.into();
    }

    /// Set the text returned by get_text().
    pub fn set_text(&self, text: impl Into<String>) {
        *self.text_content.lock().unwrap() = text.into();
    }

    /// Set the ARIA tree returned by get_aria_tree().
    pub fn set_aria_tree(&self, tree: impl Into<String>) {
        *self.aria_tree.lock().unwrap() = tree.into();
    }

    /// Set the screenshot bytes.
    pub fn set_screenshot(&self, bytes: Vec<u8>) {
        *self.screenshot_bytes.lock().unwrap() = bytes;
    }

    /// Add a JavaScript result for a given script.
    pub fn add_js_result(&self, script: impl Into<String>, result: Value) {
        self.js_results
            .lock()
            .unwrap()
            .insert(script.into(), result);
    }

    /// Set an error that navigate() will return.
    pub fn set_navigate_error(&self, err: BrowserError) {
        *self.navigate_error.lock().unwrap() = Some(err);
    }

    /// Set an error that click() will return.
    pub fn set_click_error(&self, err: BrowserError) {
        *self.click_error.lock().unwrap() = Some(err);
    }

    /// Set an error that wait_for_selector() will return.
    pub fn set_wait_error(&self, err: BrowserError) {
        *self.wait_error.lock().unwrap() = Some(err);
    }

    fn log_call(&self, method: &str, args: Vec<String>) {
        self.call_log
            .lock()
            .unwrap()
            .push((method.to_string(), args));
    }

    /// Get the number of calls to a given method.
    pub fn call_count(&self, method: &str) -> usize {
        self.call_log
            .lock()
            .unwrap()
            .iter()
            .filter(|(m, _)| m == method)
            .count()
    }

    /// Get all recorded calls.
    pub fn calls(&self) -> Vec<(String, Vec<String>)> {
        self.call_log.lock().unwrap().clone()
    }
}

#[async_trait]
impl CdpClient for MockCdpClient {
    async fn navigate(&self, url: &str) -> Result<(), BrowserError> {
        self.log_call("navigate", vec![url.to_string()]);
        if let Some(err) = self.navigate_error.lock().unwrap().take() {
            return Err(err);
        }
        *self.current_url.lock().unwrap() = url.to_string();
        Ok(())
    }

    async fn go_back(&self) -> Result<(), BrowserError> {
        self.log_call("go_back", vec![]);
        Ok(())
    }

    async fn go_forward(&self) -> Result<(), BrowserError> {
        self.log_call("go_forward", vec![]);
        Ok(())
    }

    async fn refresh(&self) -> Result<(), BrowserError> {
        self.log_call("refresh", vec![]);
        Ok(())
    }

    async fn click(&self, selector: &str) -> Result<(), BrowserError> {
        self.log_call("click", vec![selector.to_string()]);
        if let Some(err) = self.click_error.lock().unwrap().take() {
            return Err(err);
        }
        Ok(())
    }

    async fn type_text(&self, selector: &str, text: &str) -> Result<(), BrowserError> {
        self.log_call("type_text", vec![selector.to_string(), text.to_string()]);
        Ok(())
    }

    async fn fill(&self, selector: &str, value: &str) -> Result<(), BrowserError> {
        self.log_call("fill", vec![selector.to_string(), value.to_string()]);
        Ok(())
    }

    async fn select_option(&self, selector: &str, value: &str) -> Result<(), BrowserError> {
        self.log_call(
            "select_option",
            vec![selector.to_string(), value.to_string()],
        );
        Ok(())
    }

    async fn hover(&self, selector: &str) -> Result<(), BrowserError> {
        self.log_call("hover", vec![selector.to_string()]);
        Ok(())
    }

    async fn press_key(&self, key: &str) -> Result<(), BrowserError> {
        self.log_call("press_key", vec![key.to_string()]);
        Ok(())
    }

    async fn scroll(&self, x: i32, y: i32) -> Result<(), BrowserError> {
        self.log_call("scroll", vec![x.to_string(), y.to_string()]);
        Ok(())
    }

    async fn screenshot(&self) -> Result<Vec<u8>, BrowserError> {
        self.log_call("screenshot", vec![]);
        Ok(self.screenshot_bytes.lock().unwrap().clone())
    }

    async fn get_html(&self) -> Result<String, BrowserError> {
        self.log_call("get_html", vec![]);
        Ok(self.html_content.lock().unwrap().clone())
    }

    async fn get_text(&self) -> Result<String, BrowserError> {
        self.log_call("get_text", vec![]);
        Ok(self.text_content.lock().unwrap().clone())
    }

    async fn get_url(&self) -> Result<String, BrowserError> {
        self.log_call("get_url", vec![]);
        Ok(self.current_url.lock().unwrap().clone())
    }

    async fn get_title(&self) -> Result<String, BrowserError> {
        self.log_call("get_title", vec![]);
        Ok(self.current_title.lock().unwrap().clone())
    }

    async fn evaluate_js(&self, script: &str) -> Result<Value, BrowserError> {
        self.log_call("evaluate_js", vec![script.to_string()]);
        let results = self.js_results.lock().unwrap();
        match results.get(script) {
            Some(val) => Ok(val.clone()),
            None => Ok(Value::Null),
        }
    }

    async fn wait_for_selector(&self, selector: &str, timeout_ms: u64) -> Result<(), BrowserError> {
        self.log_call(
            "wait_for_selector",
            vec![selector.to_string(), timeout_ms.to_string()],
        );
        if let Some(err) = self.wait_error.lock().unwrap().take() {
            return Err(err);
        }
        Ok(())
    }

    async fn get_aria_tree(&self) -> Result<String, BrowserError> {
        self.log_call("get_aria_tree", vec![]);
        Ok(self.aria_tree.lock().unwrap().clone())
    }

    async fn upload_file(&self, selector: &str, path: &str) -> Result<(), BrowserError> {
        self.log_call("upload_file", vec![selector.to_string(), path.to_string()]);
        Ok(())
    }

    async fn close(&self) -> Result<(), BrowserError> {
        self.log_call("close", vec![]);
        *self.closed.lock().unwrap() = true;
        Ok(())
    }

    async fn new_tab(&self, url: &str) -> Result<String, BrowserError> {
        self.log_call("new_tab", vec![url.to_string()]);
        let mut counter = self.tab_counter.lock().unwrap();
        let tab_id = format!("tab-{}", *counter);
        *counter += 1;
        drop(counter);

        let tab = TabInfo {
            id: tab_id.clone(),
            url: url.to_string(),
            title: String::new(),
            active: false,
        };
        self.tabs.lock().unwrap().push(tab);
        Ok(tab_id)
    }

    async fn list_tabs(&self) -> Result<Vec<TabInfo>, BrowserError> {
        self.log_call("list_tabs", vec![]);
        let active = self.active_tab.lock().unwrap().clone();
        let mut tabs = self.tabs.lock().unwrap().clone();
        for tab in &mut tabs {
            tab.active = tab.id == active;
        }
        Ok(tabs)
    }

    async fn switch_tab(&self, tab_id: &str) -> Result<(), BrowserError> {
        self.log_call("switch_tab", vec![tab_id.to_string()]);
        let tabs = self.tabs.lock().unwrap();
        if !tabs.iter().any(|t| t.id == tab_id) {
            return Err(BrowserError::TabNotFound {
                tab_id: tab_id.to_string(),
            });
        }
        drop(tabs);
        *self.active_tab.lock().unwrap() = tab_id.to_string();
        Ok(())
    }

    async fn close_tab(&self, tab_id: &str) -> Result<(), BrowserError> {
        self.log_call("close_tab", vec![tab_id.to_string()]);
        let mut tabs = self.tabs.lock().unwrap();
        let initial_len = tabs.len();
        tabs.retain(|t| t.id != tab_id);
        if tabs.len() == initial_len {
            return Err(BrowserError::TabNotFound {
                tab_id: tab_id.to_string(),
            });
        }
        Ok(())
    }

    async fn active_tab_id(&self) -> Result<String, BrowserError> {
        self.log_call("active_tab_id", vec![]);
        Ok(self.active_tab.lock().unwrap().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_navigate() {
        let client = MockCdpClient::new();
        client.navigate("https://example.com").await.unwrap();
        assert_eq!(*client.current_url.lock().unwrap(), "https://example.com");
        assert_eq!(client.call_count("navigate"), 1);
    }

    #[tokio::test]
    async fn test_mock_navigate_error() {
        let client = MockCdpClient::new();
        client.set_navigate_error(BrowserError::NavigationFailed {
            message: "timeout".to_string(),
        });
        let result = client.navigate("https://example.com").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_click() {
        let client = MockCdpClient::new();
        client.click("#submit").await.unwrap();
        assert_eq!(client.call_count("click"), 1);
        let calls = client.calls();
        assert_eq!(calls[0].1[0], "#submit");
    }

    #[tokio::test]
    async fn test_mock_click_error() {
        let client = MockCdpClient::new();
        client.set_click_error(BrowserError::ElementNotFound {
            selector: "#missing".to_string(),
        });
        let result = client.click("#missing").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_type_text() {
        let client = MockCdpClient::new();
        client.type_text("#input", "hello").await.unwrap();
        assert_eq!(client.call_count("type_text"), 1);
    }

    #[tokio::test]
    async fn test_mock_fill() {
        let client = MockCdpClient::new();
        client.fill("#email", "user@example.com").await.unwrap();
        let calls = client.calls();
        assert_eq!(calls[0].0, "fill");
        assert_eq!(calls[0].1[1], "user@example.com");
    }

    #[tokio::test]
    async fn test_mock_screenshot() {
        let client = MockCdpClient::new();
        client.set_screenshot(vec![1, 2, 3, 4]);
        let bytes = client.screenshot().await.unwrap();
        assert_eq!(bytes, vec![1, 2, 3, 4]);
    }

    #[tokio::test]
    async fn test_mock_get_html() {
        let client = MockCdpClient::new();
        client.set_html("<html><body>Test</body></html>");
        let html = client.get_html().await.unwrap();
        assert_eq!(html, "<html><body>Test</body></html>");
    }

    #[tokio::test]
    async fn test_mock_get_text() {
        let client = MockCdpClient::new();
        client.set_text("Hello World");
        let text = client.get_text().await.unwrap();
        assert_eq!(text, "Hello World");
    }

    #[tokio::test]
    async fn test_mock_get_url_and_title() {
        let client = MockCdpClient::new();
        client.set_url("https://docs.rs");
        client.set_title("Docs.rs");
        assert_eq!(client.get_url().await.unwrap(), "https://docs.rs");
        assert_eq!(client.get_title().await.unwrap(), "Docs.rs");
    }

    #[tokio::test]
    async fn test_mock_evaluate_js() {
        let client = MockCdpClient::new();
        client.add_js_result("1+1", serde_json::json!(2));
        let result = client.evaluate_js("1+1").await.unwrap();
        assert_eq!(result, serde_json::json!(2));
    }

    #[tokio::test]
    async fn test_mock_evaluate_js_unknown_script() {
        let client = MockCdpClient::new();
        let result = client.evaluate_js("unknown()").await.unwrap();
        assert_eq!(result, Value::Null);
    }

    #[tokio::test]
    async fn test_mock_wait_for_selector() {
        let client = MockCdpClient::new();
        client.wait_for_selector("#loaded", 5000).await.unwrap();
        assert_eq!(client.call_count("wait_for_selector"), 1);
    }

    #[tokio::test]
    async fn test_mock_wait_for_selector_timeout() {
        let client = MockCdpClient::new();
        client.set_wait_error(BrowserError::Timeout { timeout_secs: 5 });
        let result = client.wait_for_selector("#never", 5000).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_aria_tree() {
        let client = MockCdpClient::new();
        client.set_aria_tree("document\n  heading 'Title'");
        let tree = client.get_aria_tree().await.unwrap();
        assert!(tree.contains("heading"));
    }

    #[tokio::test]
    async fn test_mock_close() {
        let client = MockCdpClient::new();
        assert!(!*client.closed.lock().unwrap());
        client.close().await.unwrap();
        assert!(*client.closed.lock().unwrap());
    }

    #[tokio::test]
    async fn test_mock_navigation_methods() {
        let client = MockCdpClient::new();
        client.go_back().await.unwrap();
        client.go_forward().await.unwrap();
        client.refresh().await.unwrap();
        assert_eq!(client.call_count("go_back"), 1);
        assert_eq!(client.call_count("go_forward"), 1);
        assert_eq!(client.call_count("refresh"), 1);
    }

    #[tokio::test]
    async fn test_mock_scroll() {
        let client = MockCdpClient::new();
        client.scroll(0, 500).await.unwrap();
        let calls = client.calls();
        assert_eq!(calls[0].0, "scroll");
        assert_eq!(calls[0].1, vec!["0", "500"]);
    }

    #[tokio::test]
    async fn test_mock_hover_and_press_key() {
        let client = MockCdpClient::new();
        client.hover("#menu").await.unwrap();
        client.press_key("Enter").await.unwrap();
        assert_eq!(client.call_count("hover"), 1);
        assert_eq!(client.call_count("press_key"), 1);
    }

    #[tokio::test]
    async fn test_mock_select_option() {
        let client = MockCdpClient::new();
        client.select_option("#country", "US").await.unwrap();
        assert_eq!(client.call_count("select_option"), 1);
    }

    #[tokio::test]
    async fn test_mock_upload_file() {
        let client = MockCdpClient::new();
        client
            .upload_file("#file-input", "/tmp/test.txt")
            .await
            .unwrap();
        let calls = client.calls();
        assert_eq!(calls[0].0, "upload_file");
        assert_eq!(calls[0].1[1], "/tmp/test.txt");
    }

    // --- Tab management tests ---

    #[tokio::test]
    async fn test_mock_default_has_one_tab() {
        let client = MockCdpClient::new();
        let tabs = client.list_tabs().await.unwrap();
        assert_eq!(tabs.len(), 1);
        assert_eq!(tabs[0].id, "tab-0");
        assert!(tabs[0].active);
    }

    #[tokio::test]
    async fn test_mock_new_tab() {
        let client = MockCdpClient::new();
        let tab_id = client.new_tab("https://example.com").await.unwrap();
        assert_eq!(tab_id, "tab-1");
        let tabs = client.list_tabs().await.unwrap();
        assert_eq!(tabs.len(), 2);
        assert_eq!(tabs[1].url, "https://example.com");
    }

    #[tokio::test]
    async fn test_mock_switch_tab() {
        let client = MockCdpClient::new();
        let tab_id = client.new_tab("https://example.com").await.unwrap();
        client.switch_tab(&tab_id).await.unwrap();
        assert_eq!(client.active_tab_id().await.unwrap(), tab_id);
        // The new tab should be marked active in list
        let tabs = client.list_tabs().await.unwrap();
        assert!(!tabs[0].active);
        assert!(tabs[1].active);
    }

    #[tokio::test]
    async fn test_mock_switch_tab_not_found() {
        let client = MockCdpClient::new();
        let result = client.switch_tab("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_close_tab() {
        let client = MockCdpClient::new();
        let tab_id = client.new_tab("https://example.com").await.unwrap();
        assert_eq!(client.list_tabs().await.unwrap().len(), 2);
        client.close_tab(&tab_id).await.unwrap();
        assert_eq!(client.list_tabs().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_mock_close_tab_not_found() {
        let client = MockCdpClient::new();
        let result = client.close_tab("nonexistent").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_active_tab_id() {
        let client = MockCdpClient::new();
        assert_eq!(client.active_tab_id().await.unwrap(), "tab-0");
    }

    #[tokio::test]
    async fn test_mock_multiple_tabs() {
        let client = MockCdpClient::new();
        let t1 = client.new_tab("https://one.com").await.unwrap();
        let t2 = client.new_tab("https://two.com").await.unwrap();
        let _t3 = client.new_tab("https://three.com").await.unwrap();
        assert_eq!(client.list_tabs().await.unwrap().len(), 4); // default + 3 new
        client.close_tab(&t1).await.unwrap();
        assert_eq!(client.list_tabs().await.unwrap().len(), 3);
        client.switch_tab(&t2).await.unwrap();
        assert_eq!(client.active_tab_id().await.unwrap(), t2);
    }
}

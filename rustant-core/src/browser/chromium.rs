//! Real CDP client implementation using chromiumoxide.
//!
//! This module provides `ChromiumCdpClient`, which implements the `CdpClient`
//! trait by driving an actual Chrome/Chromium browser via the DevTools Protocol.
//!
//! Supports three connection modes:
//! - `Launch`: Start a new Chrome instance (with remote debugging port)
//! - `Connect`: Attach to an already-running Chrome instance
//! - `Auto`: Try connecting first, fall back to launching
//!
//! Requires the `browser` feature flag:
//! ```toml
//! rustant-core = { path = "rustant-core", features = ["browser"] }
//! ```

use crate::browser::cdp::TabInfo;
use crate::config::{BrowserConfig, BrowserConnectionMode};
use crate::error::BrowserError;
use async_trait::async_trait;
use futures::StreamExt;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A real CDP client backed by chromiumoxide.
///
/// Manages a Chrome/Chromium browser process and multiple active pages/tabs.
/// All single-page trait methods operate on the currently active tab.
pub struct ChromiumCdpClient {
    browser: Arc<Mutex<chromiumoxide::Browser>>,
    pages: Arc<Mutex<HashMap<String, chromiumoxide::Page>>>,
    active_tab_id: Arc<Mutex<String>>,
    debug_port: u16,
    _handler: tokio::task::JoinHandle<()>,
}

impl ChromiumCdpClient {
    // -----------------------------------------------------------------------
    // Connection methods
    // -----------------------------------------------------------------------

    /// Connect to an existing Chrome instance, or launch a new one, depending
    /// on `BrowserConfig.connection_mode`.
    pub async fn connect_or_launch(config: &BrowserConfig) -> Result<Self, BrowserError> {
        match config.connection_mode {
            BrowserConnectionMode::Connect => {
                let url = Self::resolve_connect_url(config);
                Self::connect(&url, config.debug_port).await
            }
            BrowserConnectionMode::Launch => Self::launch_with_debugging(config).await,
            BrowserConnectionMode::Auto => {
                let url = format!("http://127.0.0.1:{}", config.debug_port);
                match Self::connect(&url, config.debug_port).await {
                    Ok(client) => {
                        tracing::info!(port = config.debug_port, "Connected to existing Chrome");
                        Ok(client)
                    }
                    Err(_) => {
                        tracing::info!("No existing Chrome found, launching new instance");
                        Self::launch_with_debugging(config).await
                    }
                }
            }
        }
    }

    /// Connect to an already-running Chrome instance at the given URL.
    pub async fn connect(url: &str, debug_port: u16) -> Result<Self, BrowserError> {
        let (browser, mut handler) =
            chromiumoxide::Browser::connect(url)
                .await
                .map_err(|e| BrowserError::SessionError {
                    message: format!("Failed to connect to Chrome at {}: {}", url, e),
                })?;

        let handler_task =
            tokio::spawn(async move { while let Some(_event) = handler.next().await {} });

        let existing_pages = browser
            .pages()
            .await
            .map_err(|e| BrowserError::SessionError {
                message: format!("Failed to enumerate pages: {}", e),
            })?;

        let mut pages_map = HashMap::new();
        let mut first_id = None;

        for page in existing_pages {
            let tab_id = format!("{:?}", page.target_id());
            if first_id.is_none() {
                first_id = Some(tab_id.clone());
            }
            pages_map.insert(tab_id, page);
        }

        let active_id =
            if let Some(id) = first_id {
                id
            } else {
                let page = browser.new_page("about:blank").await.map_err(|e| {
                    BrowserError::SessionError {
                        message: format!("Failed to create page: {}", e),
                    }
                })?;
                let id = format!("{:?}", page.target_id());
                pages_map.insert(id.clone(), page);
                id
            };

        Ok(Self {
            browser: Arc::new(Mutex::new(browser)),
            pages: Arc::new(Mutex::new(pages_map)),
            active_tab_id: Arc::new(Mutex::new(active_id)),
            debug_port,
            _handler: handler_task,
        })
    }

    /// Launch a new Chrome with `--remote-debugging-port` for reconnection.
    pub async fn launch_with_debugging(config: &BrowserConfig) -> Result<Self, BrowserError> {
        let chrome_path = find_chrome_binary(config)?;
        let mut builder = chromiumoxide::BrowserConfig::builder().chrome_executable(chrome_path);

        if config.headless {
            builder = builder.arg("--headless=new");
        }

        builder = builder.window_size(
            config.default_viewport_width,
            config.default_viewport_height,
        );

        // Use persistent user data dir if configured, otherwise temp dir
        let user_data_dir = if !config.isolate_profile {
            config.user_data_dir.clone().unwrap_or_else(|| {
                let dir = directories::ProjectDirs::from("dev", "rustant", "rustant")
                    .map(|d| d.data_dir().join("chrome-profile"))
                    .unwrap_or_else(|| std::env::temp_dir().join("rustant-chrome-profile"));
                let _ = std::fs::create_dir_all(&dir);
                dir
            })
        } else {
            std::env::temp_dir().join(format!("rustant-chrome-{}", uuid::Uuid::new_v4()))
        };
        builder = builder.user_data_dir(&user_data_dir);

        builder = builder.arg(format!("--remote-debugging-port={}", config.debug_port));

        builder = builder
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-gpu")
            .arg("--disable-extensions")
            .arg("--disable-dev-shm-usage");

        let browser_config = builder.build().map_err(|e| BrowserError::SessionError {
            message: format!("Failed to build browser config: {}", e),
        })?;

        let (browser, mut handler) = chromiumoxide::Browser::launch(browser_config)
            .await
            .map_err(|e| BrowserError::SessionError {
                message: format!("Failed to launch Chrome: {}", e),
            })?;

        let handler_task =
            tokio::spawn(async move { while let Some(_event) = handler.next().await {} });

        let page =
            browser
                .new_page("about:blank")
                .await
                .map_err(|e| BrowserError::SessionError {
                    message: format!("Failed to create page: {}", e),
                })?;

        let tab_id = format!("{:?}", page.target_id());
        let mut pages_map = HashMap::new();
        pages_map.insert(tab_id.clone(), page);

        Ok(Self {
            browser: Arc::new(Mutex::new(browser)),
            pages: Arc::new(Mutex::new(pages_map)),
            active_tab_id: Arc::new(Mutex::new(tab_id)),
            debug_port: config.debug_port,
            _handler: handler_task,
        })
    }

    /// Legacy launch method — preserved for backward compatibility.
    pub async fn launch(config: &BrowserConfig) -> Result<Self, BrowserError> {
        Self::launch_with_debugging(config).await
    }

    /// Get the debug port this client uses.
    pub fn debug_port(&self) -> u16 {
        self.debug_port
    }

    // -----------------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------------

    fn resolve_connect_url(config: &BrowserConfig) -> String {
        config
            .ws_url
            .clone()
            .unwrap_or_else(|| format!("http://127.0.0.1:{}", config.debug_port))
    }

    async fn wait_for_selector_inner(
        &self,
        selector: &str,
        timeout_ms: u64,
        start: std::time::Instant,
    ) -> Result<(), BrowserError> {
        let timeout = std::time::Duration::from_millis(timeout_ms);
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if start.elapsed() >= timeout {
                return Err(BrowserError::Timeout {
                    timeout_secs: timeout_ms / 1000,
                });
            }
            let active_id = self.active_tab_id.lock().await.clone();
            let pages = self.pages.lock().await;
            if let Some(page) = pages.get(&active_id) {
                if page.find_element(selector).await.is_ok() {
                    return Ok(());
                }
            }
        }
    }
}

// Macro to reduce boilerplate: lock pages, get active page, run expression.
macro_rules! with_active {
    ($self:expr, $page:ident, $body:expr) => {{
        let active_id = $self.active_tab_id.lock().await.clone();
        let pages = $self.pages.lock().await;
        let $page = pages
            .get(&active_id)
            .ok_or(BrowserError::TabNotFound { tab_id: active_id })?;
        $body
    }};
}

#[async_trait]
impl super::cdp::CdpClient for ChromiumCdpClient {
    async fn navigate(&self, url: &str) -> Result<(), BrowserError> {
        with_active!(self, page, {
            page.goto(url)
                .await
                .map_err(|e| BrowserError::NavigationFailed {
                    message: format!("{}", e),
                })?;
            Ok(())
        })
    }

    async fn go_back(&self) -> Result<(), BrowserError> {
        with_active!(self, page, {
            page.evaluate("window.history.back()").await.map_err(|e| {
                BrowserError::NavigationFailed {
                    message: format!("go_back: {}", e),
                }
            })?;
            Ok(())
        })
    }

    async fn go_forward(&self) -> Result<(), BrowserError> {
        with_active!(self, page, {
            page.evaluate("window.history.forward()")
                .await
                .map_err(|e| BrowserError::NavigationFailed {
                    message: format!("go_forward: {}", e),
                })?;
            Ok(())
        })
    }

    async fn refresh(&self) -> Result<(), BrowserError> {
        with_active!(self, page, {
            page.reload()
                .await
                .map_err(|e| BrowserError::NavigationFailed {
                    message: format!("refresh: {}", e),
                })?;
            Ok(())
        })
    }

    async fn click(&self, selector: &str) -> Result<(), BrowserError> {
        with_active!(self, page, {
            let element =
                page.find_element(selector)
                    .await
                    .map_err(|_| BrowserError::ElementNotFound {
                        selector: selector.to_string(),
                    })?;
            element.click().await.map_err(|e| BrowserError::CdpError {
                message: format!("click failed: {}", e),
            })?;
            Ok(())
        })
    }

    async fn type_text(&self, selector: &str, text: &str) -> Result<(), BrowserError> {
        with_active!(self, page, {
            let element =
                page.find_element(selector)
                    .await
                    .map_err(|_| BrowserError::ElementNotFound {
                        selector: selector.to_string(),
                    })?;
            element
                .type_str(text)
                .await
                .map_err(|e| BrowserError::CdpError {
                    message: format!("type_text failed: {}", e),
                })?;
            Ok(())
        })
    }

    async fn fill(&self, selector: &str, value: &str) -> Result<(), BrowserError> {
        with_active!(self, page, {
            let clear_js = format!(
                "document.querySelector('{}').value = ''",
                selector.replace('\'', "\\'")
            );
            page.evaluate(clear_js)
                .await
                .map_err(|_| BrowserError::ElementNotFound {
                    selector: selector.to_string(),
                })?;
            let element =
                page.find_element(selector)
                    .await
                    .map_err(|_| BrowserError::ElementNotFound {
                        selector: selector.to_string(),
                    })?;
            element.click().await.map_err(|e| BrowserError::CdpError {
                message: format!("fill click failed: {}", e),
            })?;
            element
                .type_str(value)
                .await
                .map_err(|e| BrowserError::CdpError {
                    message: format!("fill type failed: {}", e),
                })?;
            Ok(())
        })
    }

    async fn select_option(&self, selector: &str, value: &str) -> Result<(), BrowserError> {
        with_active!(self, page, {
            let js = format!(
                r#"(() => {{
                    const el = document.querySelector('{}');
                    if (!el) throw new Error('Element not found');
                    el.value = '{}';
                    el.dispatchEvent(new Event('change', {{ bubbles: true }}));
                }})()"#,
                selector.replace('\'', "\\'"),
                value.replace('\'', "\\'")
            );
            page.evaluate(js)
                .await
                .map_err(|e| BrowserError::CdpError {
                    message: format!("select_option failed: {}", e),
                })?;
            Ok(())
        })
    }

    async fn hover(&self, selector: &str) -> Result<(), BrowserError> {
        with_active!(self, page, {
            let element =
                page.find_element(selector)
                    .await
                    .map_err(|_| BrowserError::ElementNotFound {
                        selector: selector.to_string(),
                    })?;
            element.hover().await.map_err(|e| BrowserError::CdpError {
                message: format!("hover failed: {}", e),
            })?;
            Ok(())
        })
    }

    async fn press_key(&self, key: &str) -> Result<(), BrowserError> {
        with_active!(self, page, {
            page.evaluate(format!(
                "document.dispatchEvent(new KeyboardEvent('keydown', {{ key: '{}' }}))",
                key.replace('\'', "\\'")
            ))
            .await
            .map_err(|e| BrowserError::CdpError {
                message: format!("press_key failed: {}", e),
            })?;
            Ok(())
        })
    }

    async fn scroll(&self, x: i32, y: i32) -> Result<(), BrowserError> {
        with_active!(self, page, {
            page.evaluate(format!("window.scrollBy({}, {})", x, y))
                .await
                .map_err(|e| BrowserError::CdpError {
                    message: format!("scroll failed: {}", e),
                })?;
            Ok(())
        })
    }

    async fn screenshot(&self) -> Result<Vec<u8>, BrowserError> {
        with_active!(self, page, {
            let bytes = page
                .screenshot(
                    chromiumoxide::page::ScreenshotParams::builder()
                        .full_page(false)
                        .build(),
                )
                .await
                .map_err(|e| BrowserError::ScreenshotFailed {
                    message: format!("{}", e),
                })?;
            Ok(bytes)
        })
    }

    async fn get_html(&self) -> Result<String, BrowserError> {
        with_active!(self, page, {
            let html = page.content().await.map_err(|e| BrowserError::CdpError {
                message: format!("get_html failed: {}", e),
            })?;
            Ok(html)
        })
    }

    async fn get_text(&self) -> Result<String, BrowserError> {
        with_active!(self, page, {
            let result = page
                .evaluate("document.body.innerText")
                .await
                .map_err(|e| BrowserError::CdpError {
                    message: format!("get_text failed: {}", e),
                })?;
            match result.into_value::<String>() {
                Ok(text) => Ok(text),
                Err(_) => Ok(String::new()),
            }
        })
    }

    async fn get_url(&self) -> Result<String, BrowserError> {
        with_active!(self, page, {
            let url = page.url().await.map_err(|e| BrowserError::CdpError {
                message: format!("get_url failed: {}", e),
            })?;
            Ok(url.unwrap_or_else(|| "about:blank".to_string()))
        })
    }

    async fn get_title(&self) -> Result<String, BrowserError> {
        with_active!(self, page, {
            let title = page.get_title().await.map_err(|e| BrowserError::CdpError {
                message: format!("get_title failed: {}", e),
            })?;
            Ok(title.unwrap_or_default())
        })
    }

    async fn evaluate_js(&self, script: &str) -> Result<Value, BrowserError> {
        with_active!(self, page, {
            let result = page
                .evaluate(script)
                .await
                .map_err(|e| BrowserError::JsEvalFailed {
                    message: format!("{}", e),
                })?;
            match result.into_value::<Value>() {
                Ok(val) => Ok(val),
                Err(_) => Ok(Value::Null),
            }
        })
    }

    async fn wait_for_selector(&self, selector: &str, timeout_ms: u64) -> Result<(), BrowserError> {
        let start = std::time::Instant::now();
        self.wait_for_selector_inner(selector, timeout_ms, start)
            .await
    }

    async fn get_aria_tree(&self) -> Result<String, BrowserError> {
        with_active!(self, page, {
            let js = r#"
                (() => {
                    function walk(el, depth) {
                        let result = '';
                        const role = el.getAttribute('role') || el.tagName.toLowerCase();
                        const name = el.getAttribute('aria-label') || el.textContent?.trim()?.substring(0, 50) || '';
                        const indent = '  '.repeat(depth);
                        if (name) {
                            result += indent + role + " '" + name + "'\n";
                        } else {
                            result += indent + role + '\n';
                        }
                        for (const child of el.children) {
                            result += walk(child, depth + 1);
                        }
                        return result;
                    }
                    return walk(document.body, 0);
                })()
            "#;
            let result = page
                .evaluate(js)
                .await
                .map_err(|e| BrowserError::CdpError {
                    message: format!("get_aria_tree failed: {}", e),
                })?;
            match result.into_value::<String>() {
                Ok(tree) => Ok(tree),
                Err(_) => Ok("(empty)".to_string()),
            }
        })
    }

    async fn upload_file(&self, selector: &str, _path: &str) -> Result<(), BrowserError> {
        with_active!(self, page, {
            let _element =
                page.find_element(selector)
                    .await
                    .map_err(|_| BrowserError::ElementNotFound {
                        selector: selector.to_string(),
                    })?;
            Err(BrowserError::CdpError {
                message: "upload_file not yet supported for real browser (security restriction)"
                    .into(),
            })
        })
    }

    async fn close(&self) -> Result<(), BrowserError> {
        let mut browser = self.browser.lock().await;
        browser
            .close()
            .await
            .map_err(|e| BrowserError::SessionError {
                message: format!("Failed to close browser: {}", e),
            })?;
        self._handler.abort();
        Ok(())
    }

    // --- Tab management ---

    async fn new_tab(&self, url: &str) -> Result<String, BrowserError> {
        let browser = self.browser.lock().await;
        let page = browser
            .new_page(url)
            .await
            .map_err(|e| BrowserError::SessionError {
                message: format!("Failed to create new tab: {}", e),
            })?;
        let tab_id = format!("{:?}", page.target_id());
        let mut pages = self.pages.lock().await;
        pages.insert(tab_id.clone(), page);
        Ok(tab_id)
    }

    async fn list_tabs(&self) -> Result<Vec<TabInfo>, BrowserError> {
        let active_id = self.active_tab_id.lock().await.clone();
        let pages = self.pages.lock().await;
        let mut tabs = Vec::with_capacity(pages.len());

        for (id, page) in pages.iter() {
            let url = page
                .url()
                .await
                .unwrap_or(None)
                .unwrap_or_else(|| "about:blank".to_string());
            let title = page.get_title().await.unwrap_or(None).unwrap_or_default();
            tabs.push(TabInfo {
                id: id.clone(),
                url,
                title,
                active: *id == active_id,
            });
        }

        Ok(tabs)
    }

    async fn switch_tab(&self, tab_id: &str) -> Result<(), BrowserError> {
        let pages = self.pages.lock().await;
        if !pages.contains_key(tab_id) {
            return Err(BrowserError::TabNotFound {
                tab_id: tab_id.to_string(),
            });
        }
        drop(pages);
        *self.active_tab_id.lock().await = tab_id.to_string();
        Ok(())
    }

    async fn close_tab(&self, tab_id: &str) -> Result<(), BrowserError> {
        let mut pages = self.pages.lock().await;
        let page = pages.remove(tab_id).ok_or(BrowserError::TabNotFound {
            tab_id: tab_id.to_string(),
        })?;
        let _ = page.evaluate("window.close()").await;
        Ok(())
    }

    async fn active_tab_id(&self) -> Result<String, BrowserError> {
        Ok(self.active_tab_id.lock().await.clone())
    }
}

// ---------------------------------------------------------------------------
// Chrome binary discovery
// ---------------------------------------------------------------------------

/// Find a Chrome or Chromium binary on the system.
fn find_chrome_binary(config: &BrowserConfig) -> Result<std::path::PathBuf, BrowserError> {
    if let Some(ref path) = config.chrome_path {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }

    #[cfg(target_os = "macos")]
    {
        let candidates = [
            "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
            "/Applications/Chromium.app/Contents/MacOS/Chromium",
            "/Applications/Google Chrome Canary.app/Contents/MacOS/Google Chrome Canary",
            "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
        ];
        for candidate in &candidates {
            let p = std::path::PathBuf::from(candidate);
            if p.exists() {
                return Ok(p);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        let candidates = [
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/snap/bin/chromium",
        ];
        for candidate in &candidates {
            let p = std::path::PathBuf::from(candidate);
            if p.exists() {
                return Ok(p);
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        let candidates = [
            r"C:\Program Files\Google\Chrome\Application\chrome.exe",
            r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe",
        ];
        for candidate in &candidates {
            let p = std::path::PathBuf::from(candidate);
            if p.exists() {
                return Ok(p);
            }
        }
    }

    Err(BrowserError::NotConnected)
}

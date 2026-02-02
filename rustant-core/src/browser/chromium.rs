//! Real CDP client implementation using chromiumoxide.
//!
//! This module provides `ChromiumCdpClient`, which implements the `CdpClient`
//! trait by driving an actual Chrome/Chromium browser via the DevTools Protocol.
//!
//! Requires the `browser` feature flag:
//! ```toml
//! rustant-core = { path = "rustant-core", features = ["browser"] }
//! ```

use crate::config::BrowserConfig;
use crate::error::BrowserError;
use async_trait::async_trait;
use futures::StreamExt;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A real CDP client backed by chromiumoxide.
///
/// Manages a Chrome/Chromium browser process and a single active page.
pub struct ChromiumCdpClient {
    page: Arc<Mutex<chromiumoxide::Page>>,
    browser: Arc<Mutex<chromiumoxide::Browser>>,
    _handler: tokio::task::JoinHandle<()>,
}

impl ChromiumCdpClient {
    /// Launch a new Chrome/Chromium browser and return a connected client.
    ///
    /// Uses `BrowserConfig` to determine headless mode, viewport size, and
    /// Chrome binary location.
    pub async fn launch(config: &BrowserConfig) -> Result<Self, BrowserError> {
        let chrome_path = find_chrome_binary(config)?;

        let mut builder = chromiumoxide::BrowserConfig::builder().chrome_executable(chrome_path);

        if config.headless {
            builder = builder.arg("--headless=new");
        }

        builder = builder.window_size(
            config.default_viewport_width,
            config.default_viewport_height,
        );

        // Use a unique temporary user-data-dir to allow parallel instances
        let user_data_dir = std::env::temp_dir().join(format!(
            "rustant-chrome-{}",
            std::process::id() as u64 * 1000
                + std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .subsec_nanos() as u64
        ));
        builder = builder.user_data_dir(user_data_dir);

        // Common stability args
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

        // Spawn the CDP event handler in the background
        let handler_task = tokio::spawn(async move {
            while let Some(_event) = handler.next().await {
                // Process CDP events
            }
        });

        let page =
            browser
                .new_page("about:blank")
                .await
                .map_err(|e| BrowserError::SessionError {
                    message: format!("Failed to create page: {}", e),
                })?;

        Ok(Self {
            page: Arc::new(Mutex::new(page)),
            browser: Arc::new(Mutex::new(browser)),
            _handler: handler_task,
        })
    }
}

#[async_trait]
impl super::cdp::CdpClient for ChromiumCdpClient {
    async fn navigate(&self, url: &str) -> Result<(), BrowserError> {
        let page = self.page.lock().await;
        page.goto(url)
            .await
            .map_err(|e| BrowserError::NavigationFailed {
                message: format!("{}", e),
            })?;
        Ok(())
    }

    async fn go_back(&self) -> Result<(), BrowserError> {
        let page = self.page.lock().await;
        page.evaluate("window.history.back()").await.map_err(|e| {
            BrowserError::NavigationFailed {
                message: format!("go_back: {}", e),
            }
        })?;
        Ok(())
    }

    async fn go_forward(&self) -> Result<(), BrowserError> {
        let page = self.page.lock().await;
        page.evaluate("window.history.forward()")
            .await
            .map_err(|e| BrowserError::NavigationFailed {
                message: format!("go_forward: {}", e),
            })?;
        Ok(())
    }

    async fn refresh(&self) -> Result<(), BrowserError> {
        let page = self.page.lock().await;
        page.reload()
            .await
            .map_err(|e| BrowserError::NavigationFailed {
                message: format!("refresh: {}", e),
            })?;
        Ok(())
    }

    async fn click(&self, selector: &str) -> Result<(), BrowserError> {
        let page = self.page.lock().await;
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
    }

    async fn type_text(&self, selector: &str, text: &str) -> Result<(), BrowserError> {
        let page = self.page.lock().await;
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
    }

    async fn fill(&self, selector: &str, value: &str) -> Result<(), BrowserError> {
        let page = self.page.lock().await;
        // Clear the field first via JS, then type the new value
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
    }

    async fn select_option(&self, selector: &str, value: &str) -> Result<(), BrowserError> {
        let page = self.page.lock().await;
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
    }

    async fn hover(&self, selector: &str) -> Result<(), BrowserError> {
        let page = self.page.lock().await;
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
    }

    async fn press_key(&self, key: &str) -> Result<(), BrowserError> {
        let page = self.page.lock().await;
        page.evaluate(format!(
            "document.dispatchEvent(new KeyboardEvent('keydown', {{ key: '{}' }}))",
            key.replace('\'', "\\'")
        ))
        .await
        .map_err(|e| BrowserError::CdpError {
            message: format!("press_key failed: {}", e),
        })?;
        Ok(())
    }

    async fn scroll(&self, x: i32, y: i32) -> Result<(), BrowserError> {
        let page = self.page.lock().await;
        page.evaluate(format!("window.scrollBy({}, {})", x, y))
            .await
            .map_err(|e| BrowserError::CdpError {
                message: format!("scroll failed: {}", e),
            })?;
        Ok(())
    }

    async fn screenshot(&self) -> Result<Vec<u8>, BrowserError> {
        let page = self.page.lock().await;
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
    }

    async fn get_html(&self) -> Result<String, BrowserError> {
        let page = self.page.lock().await;
        let html = page.content().await.map_err(|e| BrowserError::CdpError {
            message: format!("get_html failed: {}", e),
        })?;
        Ok(html)
    }

    async fn get_text(&self) -> Result<String, BrowserError> {
        let page = self.page.lock().await;
        let result = page
            .evaluate("document.body.innerText")
            .await
            .map_err(|e| BrowserError::CdpError {
                message: format!("get_text failed: {}", e),
            })?;
        // The result is a JSON value; extract the string
        match result.into_value::<String>() {
            Ok(text) => Ok(text),
            Err(_) => Ok(String::new()),
        }
    }

    async fn get_url(&self) -> Result<String, BrowserError> {
        let page = self.page.lock().await;
        let url = page.url().await.map_err(|e| BrowserError::CdpError {
            message: format!("get_url failed: {}", e),
        })?;
        Ok(url.unwrap_or_else(|| "about:blank".to_string()))
    }

    async fn get_title(&self) -> Result<String, BrowserError> {
        let page = self.page.lock().await;
        let title = page.get_title().await.map_err(|e| BrowserError::CdpError {
            message: format!("get_title failed: {}", e),
        })?;
        Ok(title.unwrap_or_default())
    }

    async fn evaluate_js(&self, script: &str) -> Result<Value, BrowserError> {
        let page = self.page.lock().await;
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
    }

    async fn wait_for_selector(&self, selector: &str, timeout_ms: u64) -> Result<(), BrowserError> {
        let start = std::time::Instant::now();
        self.wait_for_selector_inner(selector, timeout_ms, start)
            .await
    }

    async fn get_aria_tree(&self) -> Result<String, BrowserError> {
        let page = self.page.lock().await;
        // Use CDP command to get the accessibility tree
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
    }

    async fn upload_file(&self, selector: &str, path: &str) -> Result<(), BrowserError> {
        let page = self.page.lock().await;
        let element =
            page.find_element(selector)
                .await
                .map_err(|_| BrowserError::ElementNotFound {
                    selector: selector.to_string(),
                })?;
        // Use the CDP DOM.setFileInputFiles command via JS workaround
        // chromiumoxide doesn't expose setFileInputFiles directly on Element,
        // so we use the description to get the backend node ID and execute CDP.
        let _ = element;
        let _ = path;
        // For now, use JS to set the file path (limited by browser security)
        Err(BrowserError::CdpError {
            message: "upload_file not yet supported for real browser (security restriction)".into(),
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
}

impl ChromiumCdpClient {
    /// Internal helper for wait_for_selector to handle async lock/unlock in loop.
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
            let page = self.page.lock().await;
            if page.find_element(selector).await.is_ok() {
                return Ok(());
            }
        }
    }
}

/// Find a Chrome or Chromium binary on the system.
fn find_chrome_binary(config: &BrowserConfig) -> Result<std::path::PathBuf, BrowserError> {
    // 1. User-specified path from config
    if let Some(ref path) = config.chrome_path {
        let p = std::path::PathBuf::from(path);
        if p.exists() {
            return Ok(p);
        }
    }

    // 2. macOS default locations
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

    // 3. Linux default locations
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

    // 4. Windows default locations
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

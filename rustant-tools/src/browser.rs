//! Browser automation tools — 20 tools wrapping the CdpClient trait.
//!
//! All tools share a `BrowserToolContext` holding an `Arc<dyn CdpClient>` and
//! an `Arc<BrowserSecurityGuard>` for security enforcement.

use async_trait::async_trait;
use rustant_core::browser::{BrowserSecurityGuard, CdpClient, SnapshotMode};
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use std::sync::Arc;
use std::time::Duration;

use crate::registry::Tool;

/// Shared context for all browser tools.
#[derive(Clone)]
pub struct BrowserToolContext {
    pub client: Arc<dyn CdpClient>,
    pub security: Arc<BrowserSecurityGuard>,
}

impl BrowserToolContext {
    pub fn new(client: Arc<dyn CdpClient>, security: Arc<BrowserSecurityGuard>) -> Self {
        Self { client, security }
    }
}

// ---------------------------------------------------------------------------
// Helper to convert BrowserError → ToolError
// ---------------------------------------------------------------------------
fn browser_err(name: &str, e: impl std::fmt::Display) -> ToolError {
    ToolError::ExecutionFailed {
        name: name.to_string(),
        message: e.to_string(),
    }
}

fn missing_arg(tool: &str, param: &str) -> ToolError {
    ToolError::InvalidArguments {
        name: tool.to_string(),
        reason: format!("missing required '{}' parameter", param),
    }
}

// ============================================================================
// 1. browser_navigate
// ============================================================================
pub struct BrowserNavigateTool {
    ctx: BrowserToolContext,
}

impl BrowserNavigateTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}

#[async_trait]
impl Tool for BrowserNavigateTool {
    fn name(&self) -> &str {
        "browser_navigate"
    }
    fn description(&self) -> &str {
        "Navigate the browser to a URL."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "url": { "type": "string", "description": "The URL to navigate to" }
            },
            "required": ["url"]
        })
    }
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let url = args["url"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_navigate", "url"))?;
        self.ctx
            .security
            .check_url(url)
            .map_err(|e| browser_err("browser_navigate", e))?;
        self.ctx
            .client
            .navigate(url)
            .await
            .map_err(|e| browser_err("browser_navigate", e))?;
        Ok(ToolOutput::text(format!("Navigated to {}", url)))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }
}

// ============================================================================
// 2. browser_back
// ============================================================================
pub struct BrowserBackTool {
    ctx: BrowserToolContext,
}
impl BrowserBackTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserBackTool {
    fn name(&self) -> &str {
        "browser_back"
    }
    fn description(&self) -> &str {
        "Go back in browser history."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        self.ctx
            .client
            .go_back()
            .await
            .map_err(|e| browser_err("browser_back", e))?;
        Ok(ToolOutput::text("Navigated back"))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

// ============================================================================
// 3. browser_forward
// ============================================================================
pub struct BrowserForwardTool {
    ctx: BrowserToolContext,
}
impl BrowserForwardTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserForwardTool {
    fn name(&self) -> &str {
        "browser_forward"
    }
    fn description(&self) -> &str {
        "Go forward in browser history."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        self.ctx
            .client
            .go_forward()
            .await
            .map_err(|e| browser_err("browser_forward", e))?;
        Ok(ToolOutput::text("Navigated forward"))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

// ============================================================================
// 4. browser_refresh
// ============================================================================
pub struct BrowserRefreshTool {
    ctx: BrowserToolContext,
}
impl BrowserRefreshTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserRefreshTool {
    fn name(&self) -> &str {
        "browser_refresh"
    }
    fn description(&self) -> &str {
        "Refresh the current page."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        self.ctx
            .client
            .refresh()
            .await
            .map_err(|e| browser_err("browser_refresh", e))?;
        Ok(ToolOutput::text("Page refreshed"))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

// ============================================================================
// 5. browser_click
// ============================================================================
pub struct BrowserClickTool {
    ctx: BrowserToolContext,
}
impl BrowserClickTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserClickTool {
    fn name(&self) -> &str {
        "browser_click"
    }
    fn description(&self) -> &str {
        "Click an element matching a CSS selector."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string", "description": "CSS selector of the element to click" }
            },
            "required": ["selector"]
        })
    }
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let selector = args["selector"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_click", "selector"))?;
        self.ctx
            .client
            .click(selector)
            .await
            .map_err(|e| browser_err("browser_click", e))?;
        Ok(ToolOutput::text(format!("Clicked '{}'", selector)))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

// ============================================================================
// 6. browser_type
// ============================================================================
pub struct BrowserTypeTool {
    ctx: BrowserToolContext,
}
impl BrowserTypeTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserTypeTool {
    fn name(&self) -> &str {
        "browser_type"
    }
    fn description(&self) -> &str {
        "Type text into an element matching a CSS selector."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string", "description": "CSS selector" },
                "text": { "type": "string", "description": "Text to type" }
            },
            "required": ["selector", "text"]
        })
    }
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let selector = args["selector"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_type", "selector"))?;
        let text = args["text"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_type", "text"))?;
        self.ctx
            .client
            .type_text(selector, text)
            .await
            .map_err(|e| browser_err("browser_type", e))?;
        Ok(ToolOutput::text(format!(
            "Typed '{}' into '{}'",
            text, selector
        )))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

// ============================================================================
// 7. browser_fill
// ============================================================================
pub struct BrowserFillTool {
    ctx: BrowserToolContext,
}
impl BrowserFillTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserFillTool {
    fn name(&self) -> &str {
        "browser_fill"
    }
    fn description(&self) -> &str {
        "Clear a form field and fill it with a new value."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string", "description": "CSS selector of the input" },
                "value": { "type": "string", "description": "Value to fill in" }
            },
            "required": ["selector", "value"]
        })
    }
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let selector = args["selector"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_fill", "selector"))?;
        let value = args["value"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_fill", "value"))?;
        self.ctx
            .client
            .fill(selector, value)
            .await
            .map_err(|e| browser_err("browser_fill", e))?;
        Ok(ToolOutput::text(format!(
            "Filled '{}' with '{}'",
            selector, value
        )))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

// ============================================================================
// 8. browser_select
// ============================================================================
pub struct BrowserSelectTool {
    ctx: BrowserToolContext,
}
impl BrowserSelectTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserSelectTool {
    fn name(&self) -> &str {
        "browser_select"
    }
    fn description(&self) -> &str {
        "Select an option in a <select> element."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string", "description": "CSS selector of the select element" },
                "value": { "type": "string", "description": "Option value to select" }
            },
            "required": ["selector", "value"]
        })
    }
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let selector = args["selector"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_select", "selector"))?;
        let value = args["value"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_select", "value"))?;
        self.ctx
            .client
            .select_option(selector, value)
            .await
            .map_err(|e| browser_err("browser_select", e))?;
        Ok(ToolOutput::text(format!(
            "Selected '{}' in '{}'",
            value, selector
        )))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

// ============================================================================
// 9. browser_scroll
// ============================================================================
pub struct BrowserScrollTool {
    ctx: BrowserToolContext,
}
impl BrowserScrollTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserScrollTool {
    fn name(&self) -> &str {
        "browser_scroll"
    }
    fn description(&self) -> &str {
        "Scroll the page by the given x and y pixel offsets."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "x": { "type": "integer", "description": "Horizontal scroll offset", "default": 0 },
                "y": { "type": "integer", "description": "Vertical scroll offset", "default": 0 }
            }
        })
    }
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let x = args["x"].as_i64().unwrap_or(0) as i32;
        let y = args["y"].as_i64().unwrap_or(0) as i32;
        self.ctx
            .client
            .scroll(x, y)
            .await
            .map_err(|e| browser_err("browser_scroll", e))?;
        Ok(ToolOutput::text(format!("Scrolled by ({}, {})", x, y)))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

// ============================================================================
// 10. browser_hover
// ============================================================================
pub struct BrowserHoverTool {
    ctx: BrowserToolContext,
}
impl BrowserHoverTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserHoverTool {
    fn name(&self) -> &str {
        "browser_hover"
    }
    fn description(&self) -> &str {
        "Hover over an element matching a CSS selector."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string", "description": "CSS selector" }
            },
            "required": ["selector"]
        })
    }
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let selector = args["selector"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_hover", "selector"))?;
        self.ctx
            .client
            .hover(selector)
            .await
            .map_err(|e| browser_err("browser_hover", e))?;
        Ok(ToolOutput::text(format!("Hovered over '{}'", selector)))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

// ============================================================================
// 11. browser_press_key
// ============================================================================
pub struct BrowserPressKeyTool {
    ctx: BrowserToolContext,
}
impl BrowserPressKeyTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserPressKeyTool {
    fn name(&self) -> &str {
        "browser_press_key"
    }
    fn description(&self) -> &str {
        "Press a keyboard key (e.g., 'Enter', 'Tab', 'Escape')."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "key": { "type": "string", "description": "Key to press" }
            },
            "required": ["key"]
        })
    }
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let key = args["key"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_press_key", "key"))?;
        self.ctx
            .client
            .press_key(key)
            .await
            .map_err(|e| browser_err("browser_press_key", e))?;
        Ok(ToolOutput::text(format!("Pressed key '{}'", key)))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

// ============================================================================
// 12. browser_snapshot
// ============================================================================
pub struct BrowserSnapshotTool {
    ctx: BrowserToolContext,
}
impl BrowserSnapshotTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserSnapshotTool {
    fn name(&self) -> &str {
        "browser_snapshot"
    }
    fn description(&self) -> &str {
        "Take a snapshot of the page content in the specified mode (html, text, aria_tree)."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "mode": {
                    "type": "string",
                    "description": "Snapshot mode: html, text, or aria_tree",
                    "default": "text",
                    "enum": ["html", "text", "aria_tree"]
                }
            }
        })
    }
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let mode_str = args["mode"].as_str().unwrap_or("text");
        let mode = match mode_str {
            "html" => SnapshotMode::Html,
            "aria_tree" => SnapshotMode::AriaTree,
            _ => SnapshotMode::Text,
        };
        let content = match mode {
            SnapshotMode::Html => self
                .ctx
                .client
                .get_html()
                .await
                .map_err(|e| browser_err("browser_snapshot", e))?,
            SnapshotMode::Text => self
                .ctx
                .client
                .get_text()
                .await
                .map_err(|e| browser_err("browser_snapshot", e))?,
            SnapshotMode::AriaTree => self
                .ctx
                .client
                .get_aria_tree()
                .await
                .map_err(|e| browser_err("browser_snapshot", e))?,
            SnapshotMode::Screenshot => unreachable!(),
        };
        let masked = BrowserSecurityGuard::mask_credentials(&content);
        Ok(ToolOutput::text(masked))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

// ============================================================================
// 13. browser_url
// ============================================================================
pub struct BrowserUrlTool {
    ctx: BrowserToolContext,
}
impl BrowserUrlTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserUrlTool {
    fn name(&self) -> &str {
        "browser_url"
    }
    fn description(&self) -> &str {
        "Get the current page URL."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let url = self
            .ctx
            .client
            .get_url()
            .await
            .map_err(|e| browser_err("browser_url", e))?;
        Ok(ToolOutput::text(url))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

// ============================================================================
// 14. browser_title
// ============================================================================
pub struct BrowserTitleTool {
    ctx: BrowserToolContext,
}
impl BrowserTitleTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserTitleTool {
    fn name(&self) -> &str {
        "browser_title"
    }
    fn description(&self) -> &str {
        "Get the current page title."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let title = self
            .ctx
            .client
            .get_title()
            .await
            .map_err(|e| browser_err("browser_title", e))?;
        Ok(ToolOutput::text(title))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

// ============================================================================
// 15. browser_screenshot
// ============================================================================
pub struct BrowserScreenshotTool {
    ctx: BrowserToolContext,
}
impl BrowserScreenshotTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserScreenshotTool {
    fn name(&self) -> &str {
        "browser_screenshot"
    }
    fn description(&self) -> &str {
        "Take a screenshot of the current page and return it as base64-encoded PNG."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let bytes = self
            .ctx
            .client
            .screenshot()
            .await
            .map_err(|e| browser_err("browser_screenshot", e))?;
        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &bytes);
        Ok(ToolOutput::text(b64))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
}

// ============================================================================
// 16. browser_js_eval
// ============================================================================
pub struct BrowserJsEvalTool {
    ctx: BrowserToolContext,
}
impl BrowserJsEvalTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserJsEvalTool {
    fn name(&self) -> &str {
        "browser_js_eval"
    }
    fn description(&self) -> &str {
        "Evaluate a JavaScript expression in the page context and return the result."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "script": { "type": "string", "description": "JavaScript code to evaluate" }
            },
            "required": ["script"]
        })
    }
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let script = args["script"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_js_eval", "script"))?;
        let result = self
            .ctx
            .client
            .evaluate_js(script)
            .await
            .map_err(|e| browser_err("browser_js_eval", e))?;
        Ok(ToolOutput::text(
            serde_json::to_string_pretty(&result).unwrap_or_default(),
        ))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Execute
    }
    fn timeout(&self) -> Duration {
        Duration::from_secs(30)
    }
}

// ============================================================================
// 17. browser_wait
// ============================================================================
pub struct BrowserWaitTool {
    ctx: BrowserToolContext,
}
impl BrowserWaitTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserWaitTool {
    fn name(&self) -> &str {
        "browser_wait"
    }
    fn description(&self) -> &str {
        "Wait for an element matching a CSS selector to appear on the page."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string", "description": "CSS selector to wait for" },
                "timeout_ms": { "type": "integer", "description": "Timeout in milliseconds", "default": 5000 }
            },
            "required": ["selector"]
        })
    }
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let selector = args["selector"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_wait", "selector"))?;
        let timeout_ms = args["timeout_ms"].as_u64().unwrap_or(5000);
        self.ctx
            .client
            .wait_for_selector(selector, timeout_ms)
            .await
            .map_err(|e| browser_err("browser_wait", e))?;
        Ok(ToolOutput::text(format!("Element '{}' found", selector)))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }
    fn timeout(&self) -> Duration {
        Duration::from_secs(60)
    }
}

// ============================================================================
// 18. browser_file_upload
// ============================================================================
pub struct BrowserFileUploadTool {
    ctx: BrowserToolContext,
}
impl BrowserFileUploadTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserFileUploadTool {
    fn name(&self) -> &str {
        "browser_file_upload"
    }
    fn description(&self) -> &str {
        "Upload a file to a file input element."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string", "description": "CSS selector of the file input" },
                "path": { "type": "string", "description": "Local file path to upload" }
            },
            "required": ["selector", "path"]
        })
    }
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let selector = args["selector"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_file_upload", "selector"))?;
        let path = args["path"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_file_upload", "path"))?;
        self.ctx
            .client
            .upload_file(selector, path)
            .await
            .map_err(|e| browser_err("browser_file_upload", e))?;
        Ok(ToolOutput::text(format!(
            "Uploaded '{}' to '{}'",
            path, selector
        )))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Network
    }
}

// ============================================================================
// 19. browser_download
// ============================================================================
pub struct BrowserDownloadTool {
    ctx: BrowserToolContext,
}
impl BrowserDownloadTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserDownloadTool {
    fn name(&self) -> &str {
        "browser_download"
    }
    fn description(&self) -> &str {
        "Trigger a download by clicking a link element."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "selector": { "type": "string", "description": "CSS selector of the download link/button" }
            },
            "required": ["selector"]
        })
    }
    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let selector = args["selector"]
            .as_str()
            .ok_or_else(|| missing_arg("browser_download", "selector"))?;
        self.ctx
            .client
            .click(selector)
            .await
            .map_err(|e| browser_err("browser_download", e))?;
        Ok(ToolOutput::text(format!(
            "Download triggered via '{}'",
            selector
        )))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Network
    }
}

// ============================================================================
// 20. browser_close
// ============================================================================
pub struct BrowserCloseTool {
    ctx: BrowserToolContext,
}
impl BrowserCloseTool {
    pub fn new(ctx: BrowserToolContext) -> Self {
        Self { ctx }
    }
}
#[async_trait]
impl Tool for BrowserCloseTool {
    fn name(&self) -> &str {
        "browser_close"
    }
    fn description(&self) -> &str {
        "Close the current browser page/tab."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({"type": "object", "properties": {}})
    }
    async fn execute(&self, _args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        self.ctx
            .client
            .close()
            .await
            .map_err(|e| browser_err("browser_close", e))?;
        Ok(ToolOutput::text("Browser page closed"))
    }
    fn risk_level(&self) -> RiskLevel {
        RiskLevel::Write
    }
}

// ============================================================================
// Registration helper
// ============================================================================

/// Create all 20 browser tools for registration.
pub fn create_browser_tools(ctx: BrowserToolContext) -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(BrowserNavigateTool::new(ctx.clone())),
        Arc::new(BrowserBackTool::new(ctx.clone())),
        Arc::new(BrowserForwardTool::new(ctx.clone())),
        Arc::new(BrowserRefreshTool::new(ctx.clone())),
        Arc::new(BrowserClickTool::new(ctx.clone())),
        Arc::new(BrowserTypeTool::new(ctx.clone())),
        Arc::new(BrowserFillTool::new(ctx.clone())),
        Arc::new(BrowserSelectTool::new(ctx.clone())),
        Arc::new(BrowserScrollTool::new(ctx.clone())),
        Arc::new(BrowserHoverTool::new(ctx.clone())),
        Arc::new(BrowserPressKeyTool::new(ctx.clone())),
        Arc::new(BrowserSnapshotTool::new(ctx.clone())),
        Arc::new(BrowserUrlTool::new(ctx.clone())),
        Arc::new(BrowserTitleTool::new(ctx.clone())),
        Arc::new(BrowserScreenshotTool::new(ctx.clone())),
        Arc::new(BrowserJsEvalTool::new(ctx.clone())),
        Arc::new(BrowserWaitTool::new(ctx.clone())),
        Arc::new(BrowserFileUploadTool::new(ctx.clone())),
        Arc::new(BrowserDownloadTool::new(ctx.clone())),
        Arc::new(BrowserCloseTool::new(ctx)),
    ]
}

/// Register all browser tools into a ToolRegistry.
pub fn register_browser_tools(
    registry: &mut crate::registry::ToolRegistry,
    ctx: BrowserToolContext,
) {
    let tools = create_browser_tools(ctx);
    for tool in tools {
        if let Err(e) = registry.register(tool) {
            tracing::warn!("Failed to register browser tool: {}", e);
        }
    }
}

// ============================================================================
// Tests
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::ToolRegistry;
    use rustant_core::browser::MockCdpClient;
    use rustant_core::error::BrowserError;

    fn make_ctx() -> (BrowserToolContext, Arc<MockCdpClient>) {
        let client = Arc::new(MockCdpClient::new());
        let security = Arc::new(BrowserSecurityGuard::default());
        let ctx = BrowserToolContext::new(client.clone() as Arc<dyn CdpClient>, security);
        (ctx, client)
    }

    fn make_ctx_with_security(
        security: BrowserSecurityGuard,
    ) -> (BrowserToolContext, Arc<MockCdpClient>) {
        let client = Arc::new(MockCdpClient::new());
        let security = Arc::new(security);
        let ctx = BrowserToolContext::new(client.clone() as Arc<dyn CdpClient>, security);
        (ctx, client)
    }

    #[tokio::test]
    async fn test_navigate_tool_calls_cdp_navigate() {
        let (ctx, client) = make_ctx();
        let tool = BrowserNavigateTool::new(ctx);
        let result = tool
            .execute(serde_json::json!({"url": "https://example.com"}))
            .await
            .unwrap();
        assert!(result.content.contains("Navigated to"));
        assert_eq!(*client.current_url.lock().unwrap(), "https://example.com");
    }

    #[tokio::test]
    async fn test_navigate_tool_blocked_url() {
        let security = BrowserSecurityGuard::new(vec![], vec!["evil.com".to_string()]);
        let (ctx, _client) = make_ctx_with_security(security);
        let tool = BrowserNavigateTool::new(ctx);
        let result = tool
            .execute(serde_json::json!({"url": "https://evil.com"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_click_tool_calls_cdp_click() {
        let (ctx, client) = make_ctx();
        let tool = BrowserClickTool::new(ctx);
        tool.execute(serde_json::json!({"selector": "#submit"}))
            .await
            .unwrap();
        assert_eq!(client.call_count("click"), 1);
    }

    #[tokio::test]
    async fn test_click_tool_element_not_found() {
        let (ctx, client) = make_ctx();
        client.set_click_error(BrowserError::ElementNotFound {
            selector: "#missing".to_string(),
        });
        let tool = BrowserClickTool::new(ctx);
        let result = tool
            .execute(serde_json::json!({"selector": "#missing"}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_type_tool_calls_cdp_type() {
        let (ctx, client) = make_ctx();
        let tool = BrowserTypeTool::new(ctx);
        tool.execute(serde_json::json!({"selector": "#input", "text": "hello"}))
            .await
            .unwrap();
        assert_eq!(client.call_count("type_text"), 1);
    }

    #[tokio::test]
    async fn test_fill_tool_clears_and_types() {
        let (ctx, client) = make_ctx();
        let tool = BrowserFillTool::new(ctx);
        let result = tool
            .execute(serde_json::json!({"selector": "#email", "value": "a@b.com"}))
            .await
            .unwrap();
        assert!(result.content.contains("Filled"));
        assert_eq!(client.call_count("fill"), 1);
    }

    #[tokio::test]
    async fn test_screenshot_tool_returns_base64() {
        let (ctx, client) = make_ctx();
        client.set_screenshot(vec![0x89, 0x50, 0x4E, 0x47]);
        let tool = BrowserScreenshotTool::new(ctx);
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        // Base64 of PNG magic bytes
        assert!(!result.content.is_empty());
        // Should be valid base64
        let decoded =
            base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &result.content)
                .unwrap();
        assert_eq!(decoded, vec![0x89, 0x50, 0x4E, 0x47]);
    }

    #[tokio::test]
    async fn test_snapshot_tool_html_mode() {
        let (ctx, client) = make_ctx();
        client.set_html("<html><body>Hello</body></html>");
        let tool = BrowserSnapshotTool::new(ctx);
        let result = tool
            .execute(serde_json::json!({"mode": "html"}))
            .await
            .unwrap();
        assert!(result.content.contains("Hello"));
    }

    #[tokio::test]
    async fn test_snapshot_tool_aria_mode() {
        let (ctx, client) = make_ctx();
        client.set_aria_tree("document\n  heading 'Welcome'");
        let tool = BrowserSnapshotTool::new(ctx);
        let result = tool
            .execute(serde_json::json!({"mode": "aria_tree"}))
            .await
            .unwrap();
        assert!(result.content.contains("heading"));
    }

    #[tokio::test]
    async fn test_snapshot_tool_text_mode() {
        let (ctx, client) = make_ctx();
        client.set_text("Welcome to the page");
        let tool = BrowserSnapshotTool::new(ctx);
        let result = tool
            .execute(serde_json::json!({"mode": "text"}))
            .await
            .unwrap();
        assert_eq!(result.content, "Welcome to the page");
    }

    #[tokio::test]
    async fn test_js_eval_tool_returns_result() {
        let (ctx, client) = make_ctx();
        client.add_js_result("document.title", serde_json::json!("My Page"));
        let tool = BrowserJsEvalTool::new(ctx);
        let result = tool
            .execute(serde_json::json!({"script": "document.title"}))
            .await
            .unwrap();
        assert!(result.content.contains("My Page"));
    }

    #[tokio::test]
    async fn test_url_tool_returns_current_url() {
        let (ctx, client) = make_ctx();
        client.set_url("https://example.com/page");
        let tool = BrowserUrlTool::new(ctx);
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert_eq!(result.content, "https://example.com/page");
    }

    #[tokio::test]
    async fn test_wait_tool_times_out() {
        let (ctx, client) = make_ctx();
        client.set_wait_error(BrowserError::Timeout { timeout_secs: 5 });
        let tool = BrowserWaitTool::new(ctx);
        let result = tool
            .execute(serde_json::json!({"selector": "#never", "timeout_ms": 5000}))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_all_browser_tools_register() {
        let (ctx, _client) = make_ctx();
        let mut registry = ToolRegistry::new();
        register_browser_tools(&mut registry, ctx);
        assert_eq!(registry.len(), 20);

        // Verify no duplicate names
        let names = registry.list_names();
        let unique: std::collections::HashSet<_> = names.iter().collect();
        assert_eq!(unique.len(), 20);
    }

    #[tokio::test]
    async fn test_browser_tool_risk_levels() {
        let (ctx, _client) = make_ctx();
        let tools = create_browser_tools(ctx);
        let mut risk_map = std::collections::HashMap::new();
        for tool in &tools {
            risk_map.insert(tool.name().to_string(), tool.risk_level());
        }
        // Read-only tools
        assert_eq!(risk_map["browser_snapshot"], RiskLevel::ReadOnly);
        assert_eq!(risk_map["browser_url"], RiskLevel::ReadOnly);
        assert_eq!(risk_map["browser_title"], RiskLevel::ReadOnly);
        assert_eq!(risk_map["browser_screenshot"], RiskLevel::ReadOnly);
        assert_eq!(risk_map["browser_wait"], RiskLevel::ReadOnly);
        // Write tools
        assert_eq!(risk_map["browser_navigate"], RiskLevel::Write);
        assert_eq!(risk_map["browser_click"], RiskLevel::Write);
        assert_eq!(risk_map["browser_type"], RiskLevel::Write);
        assert_eq!(risk_map["browser_fill"], RiskLevel::Write);
        assert_eq!(risk_map["browser_close"], RiskLevel::Write);
        // Execute tools
        assert_eq!(risk_map["browser_js_eval"], RiskLevel::Execute);
        // Network tools
        assert_eq!(risk_map["browser_file_upload"], RiskLevel::Network);
        assert_eq!(risk_map["browser_download"], RiskLevel::Network);
    }

    #[tokio::test]
    async fn test_browser_back_forward_refresh() {
        let (ctx, client) = make_ctx();
        BrowserBackTool::new(ctx.clone())
            .execute(serde_json::json!({}))
            .await
            .unwrap();
        BrowserForwardTool::new(ctx.clone())
            .execute(serde_json::json!({}))
            .await
            .unwrap();
        BrowserRefreshTool::new(ctx)
            .execute(serde_json::json!({}))
            .await
            .unwrap();
        assert_eq!(client.call_count("go_back"), 1);
        assert_eq!(client.call_count("go_forward"), 1);
        assert_eq!(client.call_count("refresh"), 1);
    }

    #[tokio::test]
    async fn test_scroll_tool() {
        let (ctx, client) = make_ctx();
        let tool = BrowserScrollTool::new(ctx);
        tool.execute(serde_json::json!({"x": 0, "y": 500}))
            .await
            .unwrap();
        let calls = client.calls();
        let scroll_call = calls.iter().find(|(m, _)| m == "scroll").unwrap();
        assert_eq!(scroll_call.1, vec!["0", "500"]);
    }

    #[tokio::test]
    async fn test_hover_tool() {
        let (ctx, client) = make_ctx();
        let tool = BrowserHoverTool::new(ctx);
        tool.execute(serde_json::json!({"selector": "#menu"}))
            .await
            .unwrap();
        assert_eq!(client.call_count("hover"), 1);
    }

    #[tokio::test]
    async fn test_press_key_tool() {
        let (ctx, client) = make_ctx();
        let tool = BrowserPressKeyTool::new(ctx);
        tool.execute(serde_json::json!({"key": "Enter"}))
            .await
            .unwrap();
        assert_eq!(client.call_count("press_key"), 1);
    }

    #[tokio::test]
    async fn test_select_tool() {
        let (ctx, client) = make_ctx();
        let tool = BrowserSelectTool::new(ctx);
        tool.execute(serde_json::json!({"selector": "#country", "value": "US"}))
            .await
            .unwrap();
        assert_eq!(client.call_count("select_option"), 1);
    }

    #[tokio::test]
    async fn test_title_tool() {
        let (ctx, client) = make_ctx();
        client.set_title("Test Page");
        let tool = BrowserTitleTool::new(ctx);
        let result = tool.execute(serde_json::json!({})).await.unwrap();
        assert_eq!(result.content, "Test Page");
    }

    #[tokio::test]
    async fn test_file_upload_tool() {
        let (ctx, client) = make_ctx();
        let tool = BrowserFileUploadTool::new(ctx);
        tool.execute(serde_json::json!({"selector": "#file", "path": "/tmp/test.txt"}))
            .await
            .unwrap();
        assert_eq!(client.call_count("upload_file"), 1);
    }

    #[tokio::test]
    async fn test_download_tool() {
        let (ctx, client) = make_ctx();
        let tool = BrowserDownloadTool::new(ctx);
        tool.execute(serde_json::json!({"selector": "#download-btn"}))
            .await
            .unwrap();
        assert_eq!(client.call_count("click"), 1);
    }

    #[tokio::test]
    async fn test_close_tool() {
        let (ctx, client) = make_ctx();
        let tool = BrowserCloseTool::new(ctx);
        tool.execute(serde_json::json!({})).await.unwrap();
        assert!(*client.closed.lock().unwrap());
    }
}

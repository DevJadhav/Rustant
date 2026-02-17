//! macOS Screen Analysis tool — OCR and text extraction from screenshots
//! using built-in macOS capabilities (Vision framework via Python3/Shortcuts).
//!
//! This tool captures screenshots and extracts text, enabling Rustant to
//! "read" what's on screen for apps with poor accessibility support.
//! macOS only.

use crate::macos::{run_command, run_osascript, sanitize_applescript_string};
use crate::registry::Tool;
use async_trait::async_trait;
use rustant_core::error::ToolError;
use rustant_core::types::{RiskLevel, ToolOutput};
use serde_json::json;
use std::time::Duration;
use tracing::debug;

const TOOL_NAME: &str = "macos_screen_analyze";

pub struct MacosScreenAnalyzeTool;

#[async_trait]
impl Tool for MacosScreenAnalyzeTool {
    fn name(&self) -> &str {
        TOOL_NAME
    }

    fn description(&self) -> &str {
        "Analyze screen content via OCR. Actions: ocr (extract text from a screenshot \
         of the screen or a specific app window), find_on_screen (find text location \
         on screen). Uses macOS Vision framework for text recognition."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["ocr", "find_on_screen"],
                    "description": "Action to perform"
                },
                "app_name": {
                    "type": "string",
                    "description": "Capture only this app's window (optional, defaults to full screen)"
                },
                "description": {
                    "type": "string",
                    "description": "Text or element to find on screen (for find_on_screen)"
                }
            },
            "required": ["action"]
        })
    }

    async fn execute(&self, args: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidArguments {
                name: TOOL_NAME.to_string(),
                reason: "missing required 'action' parameter".to_string(),
            })?;

        match action {
            "ocr" => execute_ocr(&args).await,
            "find_on_screen" => execute_find_on_screen(&args).await,
            other => Err(ToolError::InvalidArguments {
                name: TOOL_NAME.to_string(),
                reason: format!("unknown action '{other}'. Valid: ocr, find_on_screen"),
            }),
        }
    }

    fn risk_level(&self) -> RiskLevel {
        RiskLevel::ReadOnly
    }

    fn timeout(&self) -> Duration {
        Duration::from_secs(20)
    }
}

/// Capture a screenshot to a temp file and return the path.
async fn capture_screenshot(app_name: Option<&str>) -> Result<String, ToolError> {
    let tmp_path = format!("/tmp/rustant_ocr_{}.png", std::process::id());

    if let Some(app) = app_name {
        // Capture a specific app's window
        let safe_app = sanitize_applescript_string(app);
        let script = format!(
            r#"
tell application "System Events"
    tell process "{safe_app}"
        set frontmost to true
        delay 0.3
        set win_id to id of window 1
    end tell
end tell
return win_id as string
"#
        );

        match run_osascript(&script).await {
            Ok(window_id) => {
                // Use screencapture -l with window ID
                let result =
                    run_command("screencapture", &["-l", &window_id, "-x", &tmp_path]).await;
                if result.is_err() {
                    // Fallback to full screen if window capture fails
                    run_command("screencapture", &["-x", &tmp_path])
                        .await
                        .map_err(|e| ToolError::ExecutionFailed {
                            name: TOOL_NAME.to_string(),
                            message: format!("Screenshot failed: {e}"),
                        })?;
                }
            }
            Err(_) => {
                // App not found or no window, capture full screen
                run_command("screencapture", &["-x", &tmp_path])
                    .await
                    .map_err(|e| ToolError::ExecutionFailed {
                        name: TOOL_NAME.to_string(),
                        message: format!("Screenshot failed: {e}"),
                    })?;
            }
        }
    } else {
        // Capture full screen
        run_command("screencapture", &["-x", &tmp_path])
            .await
            .map_err(|e| ToolError::ExecutionFailed {
                name: TOOL_NAME.to_string(),
                message: format!("Screenshot failed: {e}"),
            })?;
    }

    Ok(tmp_path)
}

/// Extract text from an image using macOS Vision framework via Python3.
async fn extract_text_from_image(image_path: &str) -> Result<String, ToolError> {
    // Use Python3 with PyObjC Vision framework (built-in on macOS)
    let python_script = format!(
        r#"
import sys
try:
    import Vision
    import Quartz
    from Foundation import NSURL

    image_url = NSURL.fileURLWithPath_("{path}")
    image_source = Quartz.CGImageSourceCreateWithURL(image_url, None)
    if image_source is None:
        print("Error: Could not load image", file=sys.stderr)
        sys.exit(1)
    image = Quartz.CGImageSourceCreateImageAtIndex(image_source, 0, None)
    if image is None:
        print("Error: Could not create image", file=sys.stderr)
        sys.exit(1)

    request = Vision.VNRecognizeTextRequest.alloc().init()
    request.setRecognitionLevel_(Vision.VNRequestTextRecognitionLevelAccurate)
    request.setUsesLanguageCorrection_(True)

    handler = Vision.VNImageRequestHandler.alloc().initWithCGImage_options_(image, None)
    success = handler.performRequests_error_([request], None)

    results = request.results()
    if results:
        for observation in results:
            candidates = observation.topCandidates_(1)
            if candidates:
                text = candidates[0].string()
                confidence = candidates[0].confidence()
                print(f"[{{confidence:.2f}}] {{text}}")
    else:
        print("No text found in image.")
except ImportError:
    print("Vision framework not available. Install PyObjC: pip3 install pyobjc-framework-Vision pyobjc-framework-Quartz", file=sys.stderr)
    sys.exit(1)
except Exception as e:
    print(f"OCR error: {{e}}", file=sys.stderr)
    sys.exit(1)
"#,
        path = image_path
    );

    let output = tokio::process::Command::new("python3")
        .arg("-c")
        .arg(&python_script)
        .output()
        .await
        .map_err(|e| ToolError::ExecutionFailed {
            name: TOOL_NAME.to_string(),
            message: format!("Failed to run Python3 for OCR: {e}"),
        })?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if text.is_empty() {
            Ok("No text detected in the screenshot.".to_string())
        } else {
            Ok(text)
        }
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        // Fallback: try using the `shortcuts` CLI approach
        if stderr.contains("ImportError") || stderr.contains("not available") {
            extract_text_via_shortcuts(image_path).await
        } else {
            Err(ToolError::ExecutionFailed {
                name: TOOL_NAME.to_string(),
                message: format!("OCR failed: {stderr}"),
            })
        }
    }
}

/// Fallback OCR using macOS Shortcuts (requires macOS 12+).
async fn extract_text_via_shortcuts(image_path: &str) -> Result<String, ToolError> {
    // Check if a "Rustant OCR" shortcut exists
    let check = run_command("shortcuts", &["list"]).await;
    if let Ok(list) = &check
        && list.contains("Rustant OCR") {
            let result = run_command("shortcuts", &["run", "Rustant OCR", "-i", image_path]).await;
            if let Ok(text) = result {
                return Ok(text);
            }
        }

    // Final fallback — use basic screencapture text extraction via textutil
    Err(ToolError::ExecutionFailed {
        name: TOOL_NAME.to_string(),
        message: "OCR not available. Install PyObjC (pip3 install pyobjc-framework-Vision \
                  pyobjc-framework-Quartz) or create a 'Rustant OCR' shortcut in Shortcuts.app \
                  that extracts text from images."
            .to_string(),
    })
}

async fn execute_ocr(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let app_name = args["app_name"].as_str();
    debug!(app = ?app_name, "Performing OCR");

    let screenshot_path = capture_screenshot(app_name).await?;
    let text = extract_text_from_image(&screenshot_path).await?;

    // Clean up temp file
    let _ = tokio::fs::remove_file(&screenshot_path).await;

    let source = app_name
        .map(|a| format!("'{a}' window"))
        .unwrap_or_else(|| "full screen".to_string());

    // Truncate if too long
    let truncated = if text.len() > 4000 {
        format!(
            "{}...\n\n[Truncated — {} chars total]",
            &text[..4000],
            text.len()
        )
    } else {
        text
    };

    Ok(ToolOutput::text(format!(
        "OCR results from {source}:\n{truncated}"
    )))
}

async fn execute_find_on_screen(args: &serde_json::Value) -> Result<ToolOutput, ToolError> {
    let description = args["description"]
        .as_str()
        .ok_or_else(|| ToolError::InvalidArguments {
            name: TOOL_NAME.to_string(),
            reason: "missing required 'description' parameter".to_string(),
        })?;
    let app_name = args["app_name"].as_str();
    debug!(description, app = ?app_name, "Finding text on screen");

    // Capture and OCR
    let screenshot_path = capture_screenshot(app_name).await?;
    let text = extract_text_from_image(&screenshot_path).await?;
    let _ = tokio::fs::remove_file(&screenshot_path).await;

    // Search for the described text in OCR results
    let query_lower = description.to_lowercase();
    let matches: Vec<&str> = text
        .lines()
        .filter(|line| line.to_lowercase().contains(&query_lower))
        .collect();

    if matches.is_empty() {
        Ok(ToolOutput::text(format!(
            "Text '{}' not found on screen. OCR detected {} lines of text.",
            description,
            text.lines().count()
        )))
    } else {
        let mut output = format!(
            "Found '{}' in {} location(s):\n",
            description,
            matches.len()
        );
        for m in matches.iter().take(10) {
            output.push_str(&format!("  - {m}\n"));
        }
        if matches.len() > 10 {
            output.push_str(&format!("  ... and {} more\n", matches.len() - 10));
        }
        Ok(ToolOutput::text(output))
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_analyze_name() {
        let tool = MacosScreenAnalyzeTool;
        assert_eq!(tool.name(), "macos_screen_analyze");
    }

    #[test]
    fn test_screen_analyze_risk_level() {
        let tool = MacosScreenAnalyzeTool;
        assert_eq!(tool.risk_level(), RiskLevel::ReadOnly);
    }

    #[test]
    fn test_screen_analyze_timeout() {
        let tool = MacosScreenAnalyzeTool;
        assert_eq!(tool.timeout(), Duration::from_secs(20));
    }

    #[test]
    fn test_screen_analyze_schema() {
        let tool = MacosScreenAnalyzeTool;
        let schema = tool.parameters_schema();
        let props = schema["properties"].as_object().unwrap();
        assert!(props.contains_key("action"));
        assert!(props.contains_key("app_name"));
        assert!(props.contains_key("description"));
    }

    #[tokio::test]
    async fn test_screen_analyze_missing_action() {
        let tool = MacosScreenAnalyzeTool;
        let result = tool.execute(json!({})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "macos_screen_analyze");
                assert!(reason.contains("action"));
            }
            other => panic!("Expected InvalidArguments, got: {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_screen_analyze_invalid_action() {
        let tool = MacosScreenAnalyzeTool;
        let result = tool.execute(json!({"action": "bad"})).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolError::InvalidArguments { name, reason } => {
                assert_eq!(name, "macos_screen_analyze");
                assert!(reason.contains("bad"));
            }
            other => panic!("Expected InvalidArguments, got: {:?}", other),
        }
    }
}

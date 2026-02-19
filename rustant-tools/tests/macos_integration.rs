//! Integration tests for macOS native tools.
//!
//! These tests run actual macOS commands and verify real system behavior.
//! They are gated by `#[cfg(target_os = "macos")]` so they only run on macOS.

#[cfg(target_os = "macos")]
mod macos_integration {
    use rustant_tools::macos::*;
    use rustant_tools::registry::Tool;
    use serde_json::json;

    #[tokio::test]
    async fn test_system_info_version() {
        let tool = MacosSystemInfoTool;
        let result = tool.execute(json!({"info_type": "version"})).await;
        assert!(result.is_ok(), "system_info version should succeed");
        let output = result.unwrap();
        let text = output.content;
        assert!(
            text.contains("macOS")
                || text.contains("ProductName")
                || text.contains("ProductVersion"),
            "Output should contain macOS version info, got: {text}"
        );
    }

    #[tokio::test]
    async fn test_system_info_cpu() {
        let tool = MacosSystemInfoTool;
        let result = tool.execute(json!({"info_type": "cpu"})).await;
        assert!(result.is_ok());
        let text = result.unwrap().content;
        // Should contain some CPU info (Apple or Intel)
        assert!(
            text.contains("Apple") || text.contains("Intel") || text.contains("CPU"),
            "CPU info should mention processor brand, got: {text}"
        );
    }

    #[tokio::test]
    async fn test_system_info_all() {
        let tool = MacosSystemInfoTool;
        let result = tool.execute(json!({"info_type": "all"})).await;
        assert!(result.is_ok());
        let text = result.unwrap().content;
        // "all" should include multiple sections
        assert!(text.contains("##"), "Should have markdown headers");
    }

    #[tokio::test]
    async fn test_clipboard_roundtrip() {
        let tool = MacosClipboardTool;

        // Write a unique string
        let test_content = format!("rustant_test_{}", std::process::id());
        let write_result = tool
            .execute(json!({"action": "write", "content": test_content}))
            .await;
        assert!(write_result.is_ok(), "Clipboard write should succeed");

        // Read it back
        let read_result = tool.execute(json!({"action": "read"})).await;
        assert!(read_result.is_ok(), "Clipboard read should succeed");
        let text = read_result.unwrap().content;
        assert!(
            text.contains(&test_content),
            "Clipboard should contain written text, got: {text}"
        );
    }

    #[tokio::test]
    async fn test_spotlight_finds_applications() {
        let tool = MacosSpotlightTool;
        let result = tool
            .execute(json!({
                "query": "kind:application Safari",
                "limit": 5
            }))
            .await;
        assert!(result.is_ok(), "Spotlight search should succeed");
        let text = result.unwrap().content;
        // Should find at least something (Safari is always installed)
        assert!(
            text.contains("Safari") || text.contains("result"),
            "Should find Safari or return results, got: {text}"
        );
    }

    #[tokio::test]
    async fn test_list_running_apps() {
        let tool = MacosAppControlTool;
        let result = tool.execute(json!({"action": "list_running"})).await;
        assert!(result.is_ok(), "list_running should succeed");
        let text = result.unwrap().content;
        // Finder is always running on macOS
        assert!(
            text.contains("Finder"),
            "Running apps should include Finder, got: {text}"
        );
    }

    #[tokio::test]
    #[ignore = "Sends a real notification — requires Notification permissions"]
    async fn test_notification_sends() {
        let tool = MacosNotificationTool;
        let result = tool
            .execute(json!({
                "title": "Rustant Test",
                "message": "Integration test notification — please ignore"
            }))
            .await;
        assert!(result.is_ok(), "Notification should succeed");
    }

    #[tokio::test]
    #[ignore = "Opens Finder — interactive test"]
    async fn test_finder_reveal() {
        let tool = MacosFinderTool;
        let result = tool
            .execute(json!({"action": "reveal", "path": "/tmp"}))
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    #[ignore = "Takes a real screenshot"]
    async fn test_screenshot_full() {
        let tool = MacosScreenshotTool;
        let path = format!("/tmp/rustant_test_screenshot_{}.png", std::process::id());
        let result = tool.execute(json!({"path": path, "mode": "full"})).await;
        assert!(result.is_ok());
        // Verify file was created
        assert!(
            std::path::Path::new(&path).exists(),
            "Screenshot file should exist"
        );
        // Clean up
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    #[ignore = "Requires Calendar.app permissions"]
    async fn test_calendar_list() {
        let tool = MacosCalendarTool;
        let result = tool
            .execute(json!({"action": "list", "days_ahead": 1}))
            .await;
        assert!(result.is_ok(), "Calendar list should succeed");
    }

    #[tokio::test]
    #[ignore = "Requires Reminders.app permissions"]
    async fn test_reminders_list() {
        let tool = MacosRemindersTool;
        let result = tool.execute(json!({"action": "list"})).await;
        assert!(result.is_ok(), "Reminders list should succeed");
    }

    #[tokio::test]
    #[ignore = "Requires Notes.app permissions"]
    async fn test_notes_list() {
        let tool = MacosNotesTool;
        let result = tool.execute(json!({"action": "list", "limit": 5})).await;
        assert!(result.is_ok(), "Notes list should succeed");
    }

    // ── New screen automation tool integration tests ──

    #[tokio::test]
    #[ignore = "Requires Accessibility permissions"]
    async fn test_gui_scripting_list_elements() {
        use rustant_tools::gui_scripting::MacosGuiScriptingTool;
        let tool = MacosGuiScriptingTool;
        let result = tool
            .execute(json!({"action": "list_elements", "app_name": "Finder", "max_depth": 2}))
            .await;
        assert!(result.is_ok(), "list_elements on Finder should succeed");
        let text = result.unwrap().content;
        assert!(
            text.contains("AX") || text.contains("Finder"),
            "Should contain accessibility roles, got: {}",
            &text[..text.len().min(200)]
        );
    }

    #[tokio::test]
    #[ignore = "Requires Accessibility permissions"]
    async fn test_accessibility_get_frontmost() {
        use rustant_tools::accessibility::MacosAccessibilityTool;
        let tool = MacosAccessibilityTool;
        let result = tool.execute(json!({"action": "get_frontmost_app"})).await;
        assert!(result.is_ok(), "get_frontmost_app should succeed");
        let text = result.unwrap().content;
        assert!(
            text.contains("App:"),
            "Should contain 'App:' label, got: {text}"
        );
    }

    #[tokio::test]
    #[ignore = "Requires Contacts.app access"]
    async fn test_contacts_list_groups() {
        use rustant_tools::contacts::MacosContactsTool;
        let tool = MacosContactsTool;
        let result = tool.execute(json!({"action": "list_groups"})).await;
        assert!(result.is_ok(), "list_groups should succeed");
    }

    #[tokio::test]
    #[ignore = "Requires Safari to be installed"]
    async fn test_safari_list_tabs() {
        use rustant_tools::safari::MacosSafariTool;
        let tool = MacosSafariTool;
        let result = tool.execute(json!({"action": "list_tabs"})).await;
        assert!(result.is_ok(), "list_tabs should succeed");
    }

    #[tokio::test]
    #[ignore = "Requires Python3 + PyObjC Vision framework"]
    async fn test_screen_analyze_ocr() {
        use rustant_tools::screen_analyze::MacosScreenAnalyzeTool;
        let tool = MacosScreenAnalyzeTool;
        let result = tool.execute(json!({"action": "ocr"})).await;
        // OCR may fail if PyObjC is not installed, but should not panic
        match result {
            Ok(output) => {
                assert!(!output.content.is_empty(), "OCR output should not be empty");
            }
            Err(e) => {
                let err_str = format!("{e:?}");
                assert!(
                    err_str.contains("Python")
                        || err_str.contains("pyobjc")
                        || err_str.contains("Shortcut"),
                    "Should fail with Python/PyObjC/Shortcut error, got: {err_str}"
                );
            }
        }
    }
}

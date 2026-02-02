//! Integration tests for real browser automation via ChromiumCdpClient.
//!
//! These tests require Chrome/Chromium installed and are marked `#[ignore]`.
//! Run with:
//!   cargo test -p rustant-core --features browser -- --ignored browser_integration

#[cfg(feature = "browser")]
mod browser_tests {
    use rustant_core::browser::{CdpClient, ChromiumCdpClient};
    use rustant_core::config::BrowserConfig;

    fn make_config() -> BrowserConfig {
        BrowserConfig {
            enabled: true,
            headless: true,
            ..Default::default()
        }
    }

    #[tokio::test]
    #[ignore = "requires Chrome installed"]
    async fn browser_integration_navigate_to_example_com() {
        let config = make_config();
        let client = ChromiumCdpClient::launch(&config)
            .await
            .expect("Failed to launch Chrome");

        client.navigate("https://example.com").await.unwrap();

        let url = client.get_url().await.unwrap();
        assert!(
            url.contains("example.com"),
            "URL '{}' should contain 'example.com'",
            url
        );

        let title = client.get_title().await.unwrap();
        assert!(
            title.to_lowercase().contains("example"),
            "Title '{}' should contain 'example'",
            title
        );

        let text = client.get_text().await.unwrap();
        assert!(!text.is_empty(), "Page text should not be empty");

        client.close().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires Chrome installed"]
    async fn browser_integration_screenshot_returns_png() {
        let config = make_config();
        let client = ChromiumCdpClient::launch(&config).await.unwrap();
        client.navigate("https://example.com").await.unwrap();

        let bytes = client.screenshot().await.unwrap();
        // PNG magic bytes: 0x89 0x50 0x4E 0x47
        assert!(bytes.len() > 8, "Screenshot should be larger than 8 bytes");
        assert_eq!(
            &bytes[0..4],
            &[0x89, 0x50, 0x4E, 0x47],
            "Screenshot should start with PNG magic bytes"
        );

        client.close().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires Chrome installed"]
    async fn browser_integration_evaluate_js() {
        let config = make_config();
        let client = ChromiumCdpClient::launch(&config).await.unwrap();
        client.navigate("https://example.com").await.unwrap();

        let result = client.evaluate_js("1 + 1").await.unwrap();
        assert_eq!(result, serde_json::json!(2));

        // Test string result
        let result = client.evaluate_js("document.title").await.unwrap();
        let title = result.as_str().unwrap_or("");
        assert!(
            !title.is_empty(),
            "document.title should return a non-empty string"
        );

        client.close().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires Chrome installed"]
    async fn browser_integration_wait_for_selector() {
        let config = make_config();
        let client = ChromiumCdpClient::launch(&config).await.unwrap();
        client.navigate("https://example.com").await.unwrap();

        // h1 exists on example.com
        client.wait_for_selector("h1", 5000).await.unwrap();

        // Non-existent selector should timeout
        let result = client
            .wait_for_selector("#nonexistent-element-xyz", 1000)
            .await;
        assert!(
            result.is_err(),
            "Should timeout waiting for non-existent selector"
        );

        client.close().await.unwrap();
    }

    #[tokio::test]
    #[ignore = "requires Chrome installed"]
    async fn browser_integration_get_html_and_text() {
        let config = make_config();
        let client = ChromiumCdpClient::launch(&config).await.unwrap();
        client.navigate("https://example.com").await.unwrap();

        let html = client.get_html().await.unwrap();
        assert!(html.contains("<html"), "HTML should contain <html tag");

        let text = client.get_text().await.unwrap();
        assert!(
            text.contains("Example Domain"),
            "Text '{}' should contain 'Example Domain'",
            &text[..text.len().min(200)]
        );

        client.close().await.unwrap();
    }
}

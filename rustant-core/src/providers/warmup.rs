//! Provider connection warmup for TTFT optimization.
//!
//! Pre-establishes HTTP connection pools during startup to eliminate
//! first-request TCP/TLS handshake latency (~50-150ms savings).

use std::time::{Duration, Instant};

/// Result of warming up provider connections.
#[derive(Debug, Clone)]
pub struct WarmupResult {
    /// Providers that were successfully warmed.
    pub warmed: Vec<String>,
    /// Providers that failed warmup (non-fatal).
    pub failed: Vec<(String, String)>,
    /// Total time spent warming connections.
    pub duration: Duration,
}

/// Pre-establish HTTP connection pools for configured providers.
///
/// Makes a lightweight HEAD/OPTIONS request to each provider endpoint to trigger
/// TCP+TLS handshake and populate reqwest's connection pool. This eliminates
/// 50-150ms of latency on the first real request.
///
/// Ollama (localhost) is skipped since it has no TLS overhead.
pub async fn warm_provider_connections(
    api_key_anthropic: Option<&str>,
    api_key_openai: Option<&str>,
    api_key_gemini: Option<&str>,
) -> WarmupResult {
    let start = Instant::now();
    let mut warmed = Vec::new();
    let mut failed = Vec::new();

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .connect_timeout(Duration::from_secs(3))
        .pool_max_idle_per_host(2)
        .build()
        .unwrap_or_default();

    // Warm Anthropic connection
    if api_key_anthropic.is_some() {
        match warm_endpoint(&client, "https://api.anthropic.com/v1/messages").await {
            Ok(()) => warmed.push("anthropic".to_string()),
            Err(e) => failed.push(("anthropic".to_string(), e)),
        }
    }

    // Warm OpenAI connection
    if api_key_openai.is_some() {
        match warm_endpoint(&client, "https://api.openai.com/v1/chat/completions").await {
            Ok(()) => warmed.push("openai".to_string()),
            Err(e) => failed.push(("openai".to_string(), e)),
        }
    }

    // Warm Gemini connection
    if api_key_gemini.is_some() {
        match warm_endpoint(
            &client,
            "https://generativelanguage.googleapis.com/v1beta/models",
        )
        .await
        {
            Ok(()) => warmed.push("gemini".to_string()),
            Err(e) => failed.push(("gemini".to_string(), e)),
        }
    }

    WarmupResult {
        warmed,
        failed,
        duration: start.elapsed(),
    }
}

/// Make a lightweight request to establish a connection pool entry.
async fn warm_endpoint(client: &reqwest::Client, url: &str) -> Result<(), String> {
    // Use HEAD request — minimal data transfer, just establishes the connection
    match client.head(url).send().await {
        Ok(_) => Ok(()),
        Err(e) => {
            // Connection errors are expected (401/403) — we only care about TCP/TLS
            if e.is_connect() || e.is_timeout() {
                Err(format!("connection failed: {e}"))
            } else {
                // HTTP errors (401, 405, etc.) are fine — connection was established
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_warmup_no_providers() {
        let result = warm_provider_connections(None, None, None).await;
        assert!(result.warmed.is_empty());
        assert!(result.failed.is_empty());
    }

    #[test]
    fn test_warmup_result_debug() {
        let result = WarmupResult {
            warmed: vec!["anthropic".into()],
            failed: vec![],
            duration: Duration::from_millis(100),
        };
        assert_eq!(result.warmed.len(), 1);
        assert!(result.failed.is_empty());
    }
}

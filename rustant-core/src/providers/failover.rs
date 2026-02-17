//! Multi-provider failover with circuit breaker and auth profile rotation.
//!
//! Provides resilient LLM access by:
//! - Trying providers in priority order
//! - Skipping providers with open circuit breakers
//! - Rotating auth profiles on rate limit errors
//! - Automatic recovery via half-open circuit state

use crate::brain::LlmProvider;
use crate::error::LlmError;
use crate::types::{CompletionRequest, CompletionResponse, Message, StreamEvent};
use async_trait::async_trait;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Circuit Breaker
// ---------------------------------------------------------------------------

/// State of a circuit breaker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitState {
    /// Normal operation — calls are permitted.
    Closed,
    /// Too many failures — calls are blocked.
    Open { since: Instant },
    /// Recovery probe — one call is permitted to test the provider.
    HalfOpen,
}

/// A circuit breaker that trips after consecutive failures and recovers
/// after a timeout.
#[derive(Debug)]
pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: usize,
    failure_threshold: usize,
    recovery_timeout: Duration,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: usize, recovery_timeout: Duration) -> Self {
        Self {
            state: CircuitState::Closed,
            failure_count: 0,
            failure_threshold,
            recovery_timeout,
        }
    }

    /// Whether a call is currently permitted.
    pub fn is_call_permitted(&mut self) -> bool {
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open { since } => {
                if since.elapsed() >= self.recovery_timeout {
                    debug!("Circuit breaker transitioning to half-open");
                    self.state = CircuitState::HalfOpen;
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    /// Record a successful call.
    pub fn record_success(&mut self) {
        self.failure_count = 0;
        if self.state == CircuitState::HalfOpen {
            debug!("Circuit breaker closing after successful probe");
        }
        self.state = CircuitState::Closed;
    }

    /// Record a failed call.
    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        if self.failure_count >= self.failure_threshold {
            let now = Instant::now();
            warn!(
                failures = self.failure_count,
                threshold = self.failure_threshold,
                "Circuit breaker opening"
            );
            self.state = CircuitState::Open { since: now };
        }
    }

    /// Get the current state.
    pub fn state(&self) -> CircuitState {
        self.state
    }
}

// ---------------------------------------------------------------------------
// Auth Profile
// ---------------------------------------------------------------------------

/// A single set of credentials for a provider.
#[derive(Debug, Clone)]
pub struct AuthProfile {
    /// Environment variable name containing the API key.
    pub api_key_env: String,
    /// When the profile entered cooldown (rate-limited).
    cooldown_until: Option<Instant>,
    /// Cooldown duration when rate-limited.
    cooldown_duration: Duration,
}

impl AuthProfile {
    pub fn new(api_key_env: impl Into<String>) -> Self {
        Self {
            api_key_env: api_key_env.into(),
            cooldown_until: None,
            cooldown_duration: Duration::from_secs(60),
        }
    }

    pub fn with_cooldown_duration(mut self, duration: Duration) -> Self {
        self.cooldown_duration = duration;
        self
    }

    /// Whether this profile can be used right now.
    pub fn is_available(&self) -> bool {
        match self.cooldown_until {
            None => true,
            Some(until) => Instant::now() >= until,
        }
    }

    /// Put this profile into cooldown.
    pub fn trigger_cooldown(&mut self) {
        info!(
            env_var = %self.api_key_env,
            cooldown_secs = self.cooldown_duration.as_secs(),
            "Auth profile entering cooldown"
        );
        self.cooldown_until = Some(Instant::now() + self.cooldown_duration);
    }
}

// ---------------------------------------------------------------------------
// Provider Entry
// ---------------------------------------------------------------------------

/// A provider with its circuit breaker and priority.
struct ProviderEntry {
    provider: Arc<dyn LlmProvider>,
    circuit_breaker: Mutex<CircuitBreaker>,
    #[allow(dead_code)]
    priority: u8,
}

// ---------------------------------------------------------------------------
// FailoverProvider
// ---------------------------------------------------------------------------

/// An LLM provider that tries multiple backends in priority order,
/// skipping providers with open circuit breakers.
pub struct FailoverProvider {
    providers: Vec<ProviderEntry>,
}

impl FailoverProvider {
    /// Create a new FailoverProvider.
    ///
    /// Providers are tried in the order given. The first provider is the primary.
    pub fn new(
        providers: Vec<Arc<dyn LlmProvider>>,
        failure_threshold: usize,
        recovery_timeout: Duration,
    ) -> Self {
        let entries = providers
            .into_iter()
            .enumerate()
            .map(|(i, provider)| ProviderEntry {
                provider,
                circuit_breaker: Mutex::new(CircuitBreaker::new(
                    failure_threshold,
                    recovery_timeout,
                )),
                priority: i as u8,
            })
            .collect();

        Self { providers: entries }
    }

    /// Get the primary (first) provider.
    fn primary(&self) -> &dyn LlmProvider {
        &*self.providers[0].provider
    }
}

#[async_trait]
impl LlmProvider for FailoverProvider {
    async fn complete(&self, request: CompletionRequest) -> Result<CompletionResponse, LlmError> {
        let mut last_error = None;

        for (i, entry) in self.providers.iter().enumerate() {
            let mut cb = entry.circuit_breaker.lock().await;
            if !cb.is_call_permitted() {
                debug!(provider_index = i, "Skipping provider — circuit open");
                continue;
            }
            drop(cb); // release lock before making the call

            match entry.provider.complete(request.clone()).await {
                Ok(response) => {
                    let mut cb = entry.circuit_breaker.lock().await;
                    cb.record_success();
                    return Ok(response);
                }
                Err(e) => {
                    warn!(
                        provider_index = i,
                        model = entry.provider.model_name(),
                        error = %e,
                        "Provider failed, trying next"
                    );
                    let mut cb = entry.circuit_breaker.lock().await;
                    cb.record_failure();
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(LlmError::Connection {
            message: "All providers failed or circuits open".into(),
        }))
    }

    async fn complete_streaming(
        &self,
        request: CompletionRequest,
        tx: mpsc::Sender<StreamEvent>,
    ) -> Result<(), LlmError> {
        let mut last_error = None;

        for (i, entry) in self.providers.iter().enumerate() {
            let mut cb = entry.circuit_breaker.lock().await;
            if !cb.is_call_permitted() {
                debug!(provider_index = i, "Skipping provider — circuit open");
                continue;
            }
            drop(cb);

            match entry
                .provider
                .complete_streaming(request.clone(), tx.clone())
                .await
            {
                Ok(()) => {
                    let mut cb = entry.circuit_breaker.lock().await;
                    cb.record_success();
                    return Ok(());
                }
                Err(e) => {
                    warn!(
                        provider_index = i,
                        error = %e,
                        "Provider streaming failed, trying next"
                    );
                    let mut cb = entry.circuit_breaker.lock().await;
                    cb.record_failure();
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or(LlmError::Connection {
            message: "All providers failed or circuits open".into(),
        }))
    }

    fn estimate_tokens(&self, messages: &[Message]) -> usize {
        self.primary().estimate_tokens(messages)
    }

    fn context_window(&self) -> usize {
        self.primary().context_window()
    }

    fn supports_tools(&self) -> bool {
        self.primary().supports_tools()
    }

    fn cost_per_token(&self) -> (f64, f64) {
        self.primary().cost_per_token()
    }

    fn model_name(&self) -> &str {
        self.primary().model_name()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::MockLlmProvider;
    use crate::types::{CompletionResponse, Message};

    /// A provider that always fails with a given error type.
    struct AlwaysFailProvider {
        model: String,
        error: String,
    }

    impl AlwaysFailProvider {
        fn new(model: &str, error: &str) -> Self {
            Self {
                model: model.to_string(),
                error: error.to_string(),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for AlwaysFailProvider {
        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, LlmError> {
            match self.error.as_str() {
                "rate_limited" => Err(LlmError::RateLimited {
                    retry_after_secs: 5,
                }),
                "timeout" => Err(LlmError::Timeout { timeout_secs: 30 }),
                _ => Err(LlmError::Connection {
                    message: format!("Always fail: {}", self.error),
                }),
            }
        }

        async fn complete_streaming(
            &self,
            _request: CompletionRequest,
            _tx: mpsc::Sender<StreamEvent>,
        ) -> Result<(), LlmError> {
            Err(LlmError::Connection {
                message: "Always fail streaming".into(),
            })
        }

        fn estimate_tokens(&self, _messages: &[Message]) -> usize {
            100
        }
        fn context_window(&self) -> usize {
            128_000
        }
        fn supports_tools(&self) -> bool {
            true
        }
        fn cost_per_token(&self) -> (f64, f64) {
            (0.0, 0.0)
        }
        fn model_name(&self) -> &str {
            &self.model
        }
    }

    /// A provider that fails N times then succeeds.
    #[allow(dead_code)]
    struct FailNThenSucceedProvider {
        model: String,
        failures_remaining: std::sync::Mutex<usize>,
    }

    impl FailNThenSucceedProvider {
        #[allow(dead_code)]
        fn new(model: &str, failures: usize) -> Self {
            Self {
                model: model.to_string(),
                failures_remaining: std::sync::Mutex::new(failures),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for FailNThenSucceedProvider {
        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, LlmError> {
            let mut remaining = self.failures_remaining.lock().unwrap();
            if *remaining > 0 {
                *remaining -= 1;
                Err(LlmError::Connection {
                    message: "temporary failure".into(),
                })
            } else {
                Ok(MockLlmProvider::text_response("recovered"))
            }
        }

        async fn complete_streaming(
            &self,
            _request: CompletionRequest,
            _tx: mpsc::Sender<StreamEvent>,
        ) -> Result<(), LlmError> {
            Ok(())
        }

        fn estimate_tokens(&self, _messages: &[Message]) -> usize {
            100
        }
        fn context_window(&self) -> usize {
            128_000
        }
        fn supports_tools(&self) -> bool {
            true
        }
        fn cost_per_token(&self) -> (f64, f64) {
            (0.0, 0.0)
        }
        fn model_name(&self) -> &str {
            &self.model
        }
    }

    // --- Circuit Breaker Tests ---

    #[test]
    fn test_circuit_breaker_starts_closed() {
        let cb = CircuitBreaker::new(3, Duration::from_secs(60));
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    #[test]
    fn test_circuit_breaker_opens_after_threshold() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(60));
        cb.record_failure();
        cb.record_failure();
        assert_eq!(cb.state(), CircuitState::Closed); // not yet
        cb.record_failure();
        assert!(matches!(cb.state(), CircuitState::Open { .. }));
    }

    #[test]
    fn test_circuit_breaker_blocks_calls_when_open() {
        let mut cb = CircuitBreaker::new(2, Duration::from_secs(600));
        cb.record_failure();
        cb.record_failure();
        assert!(!cb.is_call_permitted());
    }

    #[test]
    fn test_circuit_breaker_half_open_after_timeout() {
        let mut cb = CircuitBreaker::new(1, Duration::from_millis(1));
        cb.record_failure();
        assert!(matches!(cb.state(), CircuitState::Open { .. }));

        // Wait for recovery timeout
        std::thread::sleep(Duration::from_millis(5));
        assert!(cb.is_call_permitted()); // transitions to HalfOpen
        assert_eq!(cb.state(), CircuitState::HalfOpen);
    }

    #[test]
    fn test_circuit_breaker_closes_on_success_in_half_open() {
        let mut cb = CircuitBreaker::new(1, Duration::from_millis(1));
        cb.record_failure();
        std::thread::sleep(Duration::from_millis(5));
        cb.is_call_permitted(); // transitions to HalfOpen
        cb.record_success();
        assert_eq!(cb.state(), CircuitState::Closed);
        assert_eq!(cb.failure_count, 0);
    }

    #[test]
    fn test_circuit_breaker_success_resets_count() {
        let mut cb = CircuitBreaker::new(3, Duration::from_secs(60));
        cb.record_failure();
        cb.record_failure();
        cb.record_success();
        assert_eq!(cb.failure_count, 0);
        assert_eq!(cb.state(), CircuitState::Closed);
    }

    // --- Auth Profile Tests ---

    #[test]
    fn test_auth_profile_initially_available() {
        let profile = AuthProfile::new("TEST_KEY");
        assert!(profile.is_available());
    }

    #[test]
    fn test_auth_profile_cooldown() {
        let mut profile =
            AuthProfile::new("TEST_KEY").with_cooldown_duration(Duration::from_millis(10));
        profile.trigger_cooldown();
        assert!(!profile.is_available());

        std::thread::sleep(Duration::from_millis(15));
        assert!(profile.is_available());
    }

    // --- FailoverProvider Tests ---

    #[tokio::test]
    async fn test_failover_primary_succeeds() {
        let primary = Arc::new(MockLlmProvider::new());
        primary.queue_response(MockLlmProvider::text_response("primary response"));

        let fallback = Arc::new(MockLlmProvider::new());
        fallback.queue_response(MockLlmProvider::text_response("fallback response"));

        let provider = FailoverProvider::new(vec![primary, fallback], 3, Duration::from_secs(60));

        let response = provider
            .complete(CompletionRequest::default())
            .await
            .unwrap();
        assert_eq!(response.message.content.as_text(), Some("primary response"));
    }

    #[tokio::test]
    async fn test_failover_to_secondary() {
        let primary: Arc<dyn LlmProvider> =
            Arc::new(AlwaysFailProvider::new("primary", "connection"));
        let fallback = Arc::new(MockLlmProvider::new());
        fallback.queue_response(MockLlmProvider::text_response("fallback response"));
        let fallback: Arc<dyn LlmProvider> = fallback;

        let provider = FailoverProvider::new(vec![primary, fallback], 3, Duration::from_secs(60));

        let response = provider
            .complete(CompletionRequest::default())
            .await
            .unwrap();
        assert_eq!(
            response.message.content.as_text(),
            Some("fallback response")
        );
    }

    #[tokio::test]
    async fn test_all_providers_fail() {
        let p1: Arc<dyn LlmProvider> = Arc::new(AlwaysFailProvider::new("p1", "connection"));
        let p2: Arc<dyn LlmProvider> = Arc::new(AlwaysFailProvider::new("p2", "timeout"));

        let provider = FailoverProvider::new(vec![p1, p2], 3, Duration::from_secs(60));

        let result = provider.complete(CompletionRequest::default()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_and_skips_provider() {
        // Primary fails with threshold=1 so circuit opens immediately
        let primary: Arc<dyn LlmProvider> =
            Arc::new(AlwaysFailProvider::new("primary", "connection"));
        let fallback = Arc::new(MockLlmProvider::new());
        // Queue enough responses for multiple calls
        for _ in 0..5 {
            fallback.queue_response(MockLlmProvider::text_response("fallback"));
        }
        let fallback: Arc<dyn LlmProvider> = fallback;

        let provider = FailoverProvider::new(
            vec![primary, fallback],
            1,                        // open after 1 failure
            Duration::from_secs(600), // long recovery so it stays open
        );

        // First call: primary fails, circuit opens, fallback succeeds
        let r1 = provider
            .complete(CompletionRequest::default())
            .await
            .unwrap();
        assert_eq!(r1.message.content.as_text(), Some("fallback"));

        // Second call: primary skipped (circuit open), fallback used directly
        let r2 = provider
            .complete(CompletionRequest::default())
            .await
            .unwrap();
        assert_eq!(r2.message.content.as_text(), Some("fallback"));
    }

    #[tokio::test]
    async fn test_failover_provider_delegates_properties() {
        let primary = Arc::new(MockLlmProvider::new());
        let provider = FailoverProvider::new(
            vec![primary as Arc<dyn LlmProvider>],
            3,
            Duration::from_secs(60),
        );

        assert_eq!(provider.model_name(), "mock-model");
        assert_eq!(provider.context_window(), 128_000);
        assert!(provider.supports_tools());
        assert_eq!(provider.cost_per_token(), (0.0, 0.0));
    }

    #[tokio::test]
    async fn test_failover_streaming() {
        let primary: Arc<dyn LlmProvider> =
            Arc::new(AlwaysFailProvider::new("primary", "connection"));
        let fallback = Arc::new(MockLlmProvider::new());
        fallback.queue_response(MockLlmProvider::text_response("streamed"));
        let fallback: Arc<dyn LlmProvider> = fallback;

        let provider = FailoverProvider::new(vec![primary, fallback], 3, Duration::from_secs(60));

        let (tx, mut rx) = mpsc::channel(32);
        provider
            .complete_streaming(CompletionRequest::default(), tx)
            .await
            .unwrap();

        let mut tokens = Vec::new();
        while let Some(event) = rx.recv().await {
            match event {
                StreamEvent::Token(t) => tokens.push(t),
                StreamEvent::Done { .. } => break,
                _ => {}
            }
        }
        assert!(!tokens.is_empty());
    }
}

//! OAuth 2.0 + PKCE authentication for LLM providers.
//!
//! Provides browser-based login flows for OpenAI, Google Gemini, and (when available)
//! Anthropic. Supports both the standard authorization code flow with PKCE and a
//! device code flow for headless/SSH environments.
//!
//! # Supported providers
//!
//! | Provider | Status | Flow |
//! |----------|--------|------|
//! | OpenAI | Fully supported | OAuth 2.0 + PKCE |
//! | Google Gemini | Supported | Google OAuth 2.0 |
//! | Anthropic | Blocked for 3rd-party | API key only |

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::future::IntoFuture;
use std::net::SocketAddr;
use tokio::sync::oneshot;
use tracing::{debug, info};

use crate::credentials::{CredentialError, CredentialStore};
use crate::error::LlmError;

// ── Types ───────────────────────────────────────────────────────────────────

/// Authentication method for a provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    /// Traditional API key authentication.
    #[default]
    ApiKey,
    /// OAuth 2.0 browser-based login.
    #[serde(rename = "oauth")]
    OAuth,
}

impl std::fmt::Display for AuthMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthMethod::ApiKey => write!(f, "api_key"),
            AuthMethod::OAuth => write!(f, "oauth"),
        }
    }
}

/// OAuth 2.0 provider configuration.
#[derive(Debug, Clone)]
pub struct OAuthProviderConfig {
    /// Internal provider name (e.g., "openai", "google").
    pub provider_name: String,
    /// OAuth client ID.
    pub client_id: String,
    /// OAuth client secret (required by confidential clients like Slack, Discord, Teams).
    /// Public clients (e.g., OpenAI PKCE-only) leave this as `None`.
    pub client_secret: Option<String>,
    /// Authorization endpoint URL.
    pub authorization_url: String,
    /// Token exchange endpoint URL.
    pub token_url: String,
    /// Requested scopes.
    pub scopes: Vec<String>,
    /// Optional audience parameter (used by OpenAI).
    pub audience: Option<String>,
    /// Whether the provider supports device code flow (for headless environments).
    pub supports_device_code: bool,
    /// Device code endpoint URL (if supported).
    pub device_code_url: Option<String>,
    /// Extra query parameters to include in the authorization URL.
    pub extra_auth_params: Vec<(String, String)>,
}

/// Stored OAuth token data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    /// The access token used for API requests.
    pub access_token: String,
    /// Optional refresh token for obtaining new access tokens.
    pub refresh_token: Option<String>,
    /// Optional ID token (OpenID Connect).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    /// When the access token expires (if known).
    pub expires_at: Option<DateTime<Utc>>,
    /// Token type (usually "Bearer").
    pub token_type: String,
    /// Scopes granted by the authorization server.
    pub scopes: Vec<String>,
}

/// PKCE code verifier and challenge pair.
struct PkcePair {
    verifier: String,
    challenge: String,
}

/// Callback data received from the authorization server.
struct CallbackData {
    code: String,
    state: String,
}

// ── PKCE ────────────────────────────────────────────────────────────────────

/// Generate a PKCE code verifier and S256 challenge.
///
/// The verifier is a random 43-character string using unreserved URI characters.
/// The challenge is the base64url-encoded SHA-256 hash of the verifier.
fn generate_pkce_pair() -> PkcePair {
    let mut rng = rand::thread_rng();
    let verifier: String = (0..43)
        .map(|_| {
            const CHARSET: &[u8] =
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect();

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let digest = hasher.finalize();
    let challenge = URL_SAFE_NO_PAD.encode(digest);

    PkcePair {
        verifier,
        challenge,
    }
}

/// Generate a random state parameter for CSRF protection.
fn generate_state() -> String {
    let mut rng = rand::thread_rng();
    let bytes: [u8; 32] = rng.gen();
    URL_SAFE_NO_PAD.encode(bytes)
}

// ── Callback Server ─────────────────────────────────────────────────────────

/// Default port for the OAuth callback server.
///
/// Providers like Slack require the redirect URI to exactly match one registered
/// in the app settings. Using a fixed port ensures `https://localhost:8844/auth/callback`
/// is predictable and can be pre-configured.
pub const OAUTH_CALLBACK_PORT: u16 = 8844;

/// Build the axum router used by the OAuth callback server.
fn build_callback_router(
    tx: std::sync::Arc<tokio::sync::Mutex<Option<oneshot::Sender<CallbackData>>>>,
) -> axum::Router {
    axum::Router::new().route(
        "/auth/callback",
        axum::routing::get({
            let tx = tx.clone();
            move |query: axum::extract::Query<HashMap<String, String>>| {
                let tx = tx.clone();
                async move {
                    let code = query.get("code").cloned().unwrap_or_default();
                    let state = query.get("state").cloned().unwrap_or_default();

                    if let Some(sender) = tx.lock().await.take() {
                        let _ = sender.send(CallbackData { code, state });
                    }

                    axum::response::Html(
                        r#"<!DOCTYPE html>
<html>
<head><title>Rustant</title></head>
<body style="font-family: system-ui; text-align: center; padding-top: 80px;">
<h2>Authentication successful!</h2>
<p>You can close this tab and return to the terminal.</p>
</body>
</html>"#,
                    )
                }
            }
        }),
    )
}

/// Load TLS config for the OAuth callback server.
///
/// Tries the following in order:
/// 1. `mkcert`-generated certs in `~/.rustant/certs/` (browser-trusted)
/// 2. Falls back to a self-signed cert generated at runtime via `rcgen`
///    (the browser will show a warning on first redirect)
///
/// To generate trusted certs, run:
/// ```sh
/// mkcert -install            # installs the root CA (needs sudo)
/// mkdir -p ~/.rustant/certs
/// mkcert -cert-file ~/.rustant/certs/localhost.pem \
///        -key-file ~/.rustant/certs/localhost-key.pem \
///        localhost 127.0.0.1
/// ```
async fn load_tls_config() -> Result<axum_server::tls_rustls::RustlsConfig, LlmError> {
    // Check for mkcert certs first.
    if let Some(home) = directories::BaseDirs::new() {
        let cert_dir = home.home_dir().join(".rustant").join("certs");
        let cert_path = cert_dir.join("localhost.pem");
        let key_path = cert_dir.join("localhost-key.pem");

        if cert_path.exists() && key_path.exists() {
            info!("Using mkcert certificates from {}", cert_dir.display());
            return axum_server::tls_rustls::RustlsConfig::from_pem_file(cert_path, key_path)
                .await
                .map_err(|e| LlmError::OAuthFailed {
                    message: format!("Failed to load mkcert certificates: {}", e),
                });
        }
    }

    // Fall back to self-signed cert.
    info!(
        "No mkcert certs found in ~/.rustant/certs/. Generating self-signed certificate.\n\
         Your browser may show a security warning. To avoid this, run:\n  \
         mkcert -install && mkdir -p ~/.rustant/certs && \
         mkcert -cert-file ~/.rustant/certs/localhost.pem \
         -key-file ~/.rustant/certs/localhost-key.pem localhost 127.0.0.1"
    );

    use rcgen::CertifiedKey;
    let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string()];
    let CertifiedKey { cert, key_pair } = rcgen::generate_simple_self_signed(subject_alt_names)
        .map_err(|e| LlmError::OAuthFailed {
            message: format!("Failed to generate self-signed certificate: {}", e),
        })?;

    let cert_pem = cert.pem();
    let key_pem = key_pair.serialize_pem();

    axum_server::tls_rustls::RustlsConfig::from_pem(cert_pem.into_bytes(), key_pem.into_bytes())
        .await
        .map_err(|e| LlmError::OAuthFailed {
            message: format!("Failed to build TLS config: {}", e),
        })
}

/// Start a local callback server on the fixed port.
///
/// When `use_tls` is true, the server runs HTTPS with a self-signed certificate
/// (for providers like Slack that require HTTPS redirect URIs). When false, it
/// runs plain HTTP (suitable for OpenAI and other providers that accept HTTP on
/// localhost).
///
/// Returns the server's port and a receiver that will yield the callback data.
async fn start_callback_server(
    use_tls: bool,
) -> Result<(u16, oneshot::Receiver<CallbackData>), LlmError> {
    let (tx, rx) = oneshot::channel::<CallbackData>();
    let tx = std::sync::Arc::new(tokio::sync::Mutex::new(Some(tx)));
    let app = build_callback_router(tx);

    let bind_addr = format!("127.0.0.1:{}", OAUTH_CALLBACK_PORT);

    if use_tls {
        // Ensure the rustls CryptoProvider is installed (idempotent).
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let tls_config = load_tls_config().await?;

        let addr: SocketAddr = bind_addr.parse().map_err(|e| LlmError::OAuthFailed {
            message: format!("Invalid bind address: {}", e),
        })?;

        debug!(
            port = OAUTH_CALLBACK_PORT,
            "OAuth HTTPS callback server starting"
        );

        tokio::spawn(async move {
            let server = axum_server::bind_rustls(addr, tls_config).serve(app.into_make_service());
            let _ = tokio::time::timeout(std::time::Duration::from_secs(120), server).await;
        });

        // Give the TLS server a moment to bind.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    } else {
        let listener = tokio::net::TcpListener::bind(&bind_addr)
            .await
            .map_err(|e| LlmError::OAuthFailed {
                message: format!(
                    "Failed to bind callback server on port {}: {}. \
                     Make sure no other process is using this port.",
                    OAUTH_CALLBACK_PORT, e
                ),
            })?;

        debug!(
            port = OAUTH_CALLBACK_PORT,
            "OAuth HTTP callback server starting"
        );

        tokio::spawn(async move {
            let server = axum::serve(listener, app);
            let _ = tokio::time::timeout(std::time::Duration::from_secs(120), server.into_future())
                .await;
        });
    }

    Ok((OAUTH_CALLBACK_PORT, rx))
}

// ── Browser Flow ────────────────────────────────────────────────────────────

/// Run the full OAuth 2.0 Authorization Code flow with PKCE.
///
/// 1. Generate PKCE pair and state
/// 2. Start local callback server
/// 3. Build authorization URL and open the user's browser
/// 4. Wait for the callback with the authorization code
/// 5. Exchange the code for tokens
///
/// Returns the obtained `OAuthToken` on success.
///
/// If `redirect_uri_override` is `Some`, that URI is sent to the OAuth provider
/// instead of the default. When the redirect URI starts with `https://`, the
/// local callback server will use TLS with a self-signed certificate; otherwise
/// it runs plain HTTP.
///
/// Channel providers that require HTTPS (e.g. Slack) will automatically get a
/// TLS-enabled callback server via the `https://localhost:8844/auth/callback`
/// default.
pub async fn authorize_browser_flow(
    config: &OAuthProviderConfig,
    redirect_uri_override: Option<&str>,
) -> Result<OAuthToken, LlmError> {
    let pkce = generate_pkce_pair();
    let state = generate_state();

    // Determine the redirect URI and whether we need TLS.
    // Channel providers (Slack, Discord, etc.) require HTTPS; LLM providers
    // (OpenAI, Google) typically accept HTTP on localhost.
    let is_channel_provider = matches!(
        config.provider_name.as_str(),
        "slack" | "discord" | "teams" | "whatsapp" | "gmail"
    );

    let use_tls = match redirect_uri_override {
        Some(uri) => uri.starts_with("https://"),
        None => is_channel_provider,
    };

    // Start callback server (HTTP or HTTPS depending on use_tls).
    let (port, rx) = start_callback_server(use_tls).await?;

    let redirect_uri = match redirect_uri_override {
        Some(uri) => uri.to_string(),
        None => {
            let scheme = if use_tls { "https" } else { "http" };
            format!("{}://localhost:{}/auth/callback", scheme, port)
        }
    };

    // Build authorization URL.
    let mut auth_url =
        url::Url::parse(&config.authorization_url).map_err(|e| LlmError::OAuthFailed {
            message: format!("Invalid authorization URL: {}", e),
        })?;

    {
        let mut params = auth_url.query_pairs_mut();
        params.append_pair("response_type", "code");
        params.append_pair("client_id", &config.client_id);
        params.append_pair("redirect_uri", &redirect_uri);
        params.append_pair("code_challenge", &pkce.challenge);
        params.append_pair("code_challenge_method", "S256");
        params.append_pair("state", &state);

        if !config.scopes.is_empty() {
            params.append_pair("scope", &config.scopes.join(" "));
        }
        if let Some(ref audience) = config.audience {
            params.append_pair("audience", audience);
        }
        for (key, value) in &config.extra_auth_params {
            params.append_pair(key, value);
        }
    }

    info!("Opening browser for OAuth authorization...");
    debug!(url = %auth_url, "Authorization URL");
    open::that(auth_url.as_str()).map_err(|e| LlmError::OAuthFailed {
        message: format!("Failed to open browser: {}", e),
    })?;

    // Wait for the callback.
    let callback = tokio::time::timeout(std::time::Duration::from_secs(120), rx)
        .await
        .map_err(|_| LlmError::OAuthFailed {
            message: "OAuth callback timed out after 120 seconds".to_string(),
        })?
        .map_err(|_| LlmError::OAuthFailed {
            message: "OAuth callback channel closed unexpectedly".to_string(),
        })?;

    // Verify state parameter.
    if callback.state != state {
        return Err(LlmError::OAuthFailed {
            message: "OAuth state parameter mismatch (possible CSRF attack)".to_string(),
        });
    }

    if callback.code.is_empty() {
        return Err(LlmError::OAuthFailed {
            message: "OAuth callback did not contain an authorization code".to_string(),
        });
    }

    // Exchange authorization code for tokens.
    let mut token =
        exchange_code_for_token(config, &callback.code, &pkce.verifier, &redirect_uri).await?;

    // For OpenAI: try to exchange the ID token for a Platform API key.
    // This succeeds for accounts with Platform org/project setup. For Personal/
    // ChatGPT-only accounts it may fail — in that case we fall back to using the
    // OAuth access token directly as a Bearer token (same as Codex CLI).
    if config.provider_name == "openai" {
        if let Some(ref id_tok) = token.id_token {
            if let Some(payload) = id_tok.split('.').nth(1) {
                if let Ok(bytes) = URL_SAFE_NO_PAD.decode(payload) {
                    if let Ok(claims) = serde_json::from_slice::<serde_json::Value>(&bytes) {
                        debug!(claims = %claims, "ID token claims");
                    }
                }
            }
            debug!("Exchanging ID token for OpenAI API key...");
            match obtain_openai_api_key(config, id_tok).await {
                Ok(api_key) => {
                    info!("Obtained OpenAI Platform API key via token exchange");
                    token.access_token = api_key;
                }
                Err(e) => {
                    // The token exchange typically fails for accounts without a
                    // Platform API organization. The standard Chat Completions
                    // endpoint requires a Platform API key — the raw OAuth
                    // access token won't work.
                    return Err(LlmError::OAuthFailed {
                        message: format!(
                            "Failed to exchange OAuth token for an OpenAI API key: {}\n\n\
                             This usually means your OpenAI account does not have \
                             Platform API access set up.\n\n\
                             To fix this:\n\
                             1. Visit https://platform.openai.com to create an API organization\n\
                             2. Ensure you have a billing method or active subscription\n\
                             3. Run 'rustant auth login openai' again\n\n\
                             Alternatively, use a standard API key:\n\
                             1. Get your key from https://platform.openai.com/api-keys\n\
                             2. Set the OPENAI_API_KEY environment variable\n\
                             3. Set auth_method to empty in .rustant/config.toml",
                            e
                        ),
                    });
                }
            }
        }
    }

    Ok(token)
}

/// Exchange an authorization code for an access token.
async fn exchange_code_for_token(
    config: &OAuthProviderConfig,
    code: &str,
    code_verifier: &str,
    redirect_uri: &str,
) -> Result<OAuthToken, LlmError> {
    let client = reqwest::Client::new();

    // Build the body exactly like the Codex CLI: using urlencoding::encode()
    // with format!() and .body() for consistent percent-encoding.
    let mut body = format!(
        "grant_type={}&code={}&redirect_uri={}&client_id={}&code_verifier={}",
        urlencoding::encode("authorization_code"),
        urlencoding::encode(code),
        urlencoding::encode(redirect_uri),
        urlencoding::encode(&config.client_id),
        urlencoding::encode(code_verifier),
    );

    // Confidential clients (Slack, Discord, Teams, etc.) require a client_secret.
    if let Some(ref secret) = config.client_secret {
        body.push_str(&format!("&client_secret={}", urlencoding::encode(secret)));
    }

    debug!(provider = %config.provider_name, "Exchanging authorization code for token");

    let response = client
        .post(&config.token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| LlmError::OAuthFailed {
            message: format!("Token exchange request failed: {}", e),
        })?;

    let status = response.status();
    let body_text = response.text().await.map_err(|e| LlmError::OAuthFailed {
        message: format!("Failed to read token response: {}", e),
    })?;

    if !status.is_success() {
        return Err(LlmError::OAuthFailed {
            message: format!("Token exchange failed (HTTP {}): {}", status, body_text),
        });
    }

    parse_token_response(&body_text)
}

/// Exchange an OpenAI ID token for an actual OpenAI API key via the
/// RFC 8693 token-exchange grant type.
///
/// This is the second step of the OpenAI Codex OAuth flow: after the standard
/// PKCE code exchange, the ID token is exchanged for a usable API key.
///
/// Uses manual URL-encoded body construction (matching Codex CLI) instead of
/// `reqwest .form()` to avoid potential double-encoding of the JWT ID token.
async fn obtain_openai_api_key(
    config: &OAuthProviderConfig,
    id_token: &str,
) -> Result<String, LlmError> {
    let client = reqwest::Client::new();

    // Build the body exactly like the Codex CLI: using urlencoding::encode()
    // with format!() and .body(). This ensures identical percent-encoding
    // behavior (RFC 3986 unreserved chars, %20 for spaces).
    let body = format!(
        "grant_type={}&client_id={}&requested_token={}&subject_token={}&subject_token_type={}",
        urlencoding::encode("urn:ietf:params:oauth:grant-type:token-exchange"),
        urlencoding::encode(&config.client_id),
        urlencoding::encode("openai-api-key"),
        urlencoding::encode(id_token),
        urlencoding::encode("urn:ietf:params:oauth:token-type:id_token"),
    );

    debug!(body_len = body.len(), "Token exchange request body");

    let response = client
        .post(&config.token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| LlmError::OAuthFailed {
            message: format!("API key exchange request failed: {}", e),
        })?;

    let status = response.status();
    let body_text = response.text().await.map_err(|e| LlmError::OAuthFailed {
        message: format!("Failed to read API key exchange response: {}", e),
    })?;

    if !status.is_success() {
        return Err(LlmError::OAuthFailed {
            message: format!("API key exchange failed (HTTP {}): {}", status, body_text),
        });
    }

    let json: serde_json::Value =
        serde_json::from_str(&body_text).map_err(|e| LlmError::OAuthFailed {
            message: format!("Invalid JSON in API key exchange response: {}", e),
        })?;

    json["access_token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| LlmError::OAuthFailed {
            message: "API key exchange response missing 'access_token'".to_string(),
        })
}

/// Parse a token endpoint response into an `OAuthToken`.
fn parse_token_response(body: &str) -> Result<OAuthToken, LlmError> {
    let json: serde_json::Value =
        serde_json::from_str(body).map_err(|e| LlmError::OAuthFailed {
            message: format!("Invalid JSON in token response: {}", e),
        })?;

    let access_token = json["access_token"]
        .as_str()
        .ok_or_else(|| LlmError::OAuthFailed {
            message: "Token response missing 'access_token'".to_string(),
        })?
        .to_string();

    let refresh_token = json["refresh_token"].as_str().map(|s| s.to_string());
    let id_token = json["id_token"].as_str().map(|s| s.to_string());
    let token_type = json["token_type"].as_str().unwrap_or("Bearer").to_string();

    let expires_at = json["expires_in"]
        .as_u64()
        .map(|secs| Utc::now() + chrono::Duration::seconds(secs as i64));

    let scopes = json["scope"]
        .as_str()
        .map(|s| s.split_whitespace().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    Ok(OAuthToken {
        access_token,
        refresh_token,
        id_token,
        expires_at,
        token_type,
        scopes,
    })
}

// ── Device Code Flow ────────────────────────────────────────────────────────

/// Run the OAuth 2.0 Device Code flow for headless environments.
///
/// 1. Request a device code from the provider
/// 2. Display the user code and verification URI
/// 3. Poll the token endpoint until the user completes authorization
///
/// Returns the obtained `OAuthToken` on success.
pub async fn authorize_device_code_flow(
    config: &OAuthProviderConfig,
) -> Result<OAuthToken, LlmError> {
    let device_code_url =
        config
            .device_code_url
            .as_deref()
            .ok_or_else(|| LlmError::OAuthFailed {
                message: format!(
                    "Provider '{}' does not support device code flow",
                    config.provider_name
                ),
            })?;

    let client = reqwest::Client::new();

    // Step 1: Request device code.
    let mut params = HashMap::new();
    params.insert("client_id", config.client_id.as_str());
    if !config.scopes.is_empty() {
        let scope_str = config.scopes.join(" ");
        params.insert("scope", Box::leak(scope_str.into_boxed_str()));
    }
    if let Some(ref audience) = config.audience {
        params.insert("audience", audience.as_str());
    }

    let response = client
        .post(device_code_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| LlmError::OAuthFailed {
            message: format!("Device code request failed: {}", e),
        })?;

    let status = response.status();
    let body_text = response.text().await.map_err(|e| LlmError::OAuthFailed {
        message: format!("Failed to read device code response: {}", e),
    })?;

    if !status.is_success() {
        return Err(LlmError::OAuthFailed {
            message: format!(
                "Device code request failed (HTTP {}): {}",
                status, body_text
            ),
        });
    }

    let json: serde_json::Value =
        serde_json::from_str(&body_text).map_err(|e| LlmError::OAuthFailed {
            message: format!("Invalid JSON in device code response: {}", e),
        })?;

    let device_code = json["device_code"]
        .as_str()
        .ok_or_else(|| LlmError::OAuthFailed {
            message: "Device code response missing 'device_code'".to_string(),
        })?;
    let user_code = json["user_code"]
        .as_str()
        .ok_or_else(|| LlmError::OAuthFailed {
            message: "Device code response missing 'user_code'".to_string(),
        })?;
    let verification_uri = json["verification_uri"]
        .as_str()
        .or_else(|| json["verification_url"].as_str())
        .ok_or_else(|| LlmError::OAuthFailed {
            message: "Device code response missing 'verification_uri'".to_string(),
        })?;
    let interval = json["interval"].as_u64().unwrap_or(5);
    let expires_in = json["expires_in"].as_u64().unwrap_or(600);

    // Step 2: Display instructions.
    println!();
    println!("  To authenticate, visit: {}", verification_uri);
    println!("  Enter this code: {}", user_code);
    println!();
    println!("  Waiting for authorization...");

    // Step 3: Poll for token.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(expires_in);
    let poll_interval = std::time::Duration::from_secs(interval);

    loop {
        tokio::time::sleep(poll_interval).await;

        if tokio::time::Instant::now() >= deadline {
            return Err(LlmError::OAuthFailed {
                message: "Device code flow timed out waiting for authorization".to_string(),
            });
        }

        let mut poll_params = HashMap::new();
        poll_params.insert("grant_type", "urn:ietf:params:oauth:grant-type:device_code");
        poll_params.insert("device_code", device_code);
        poll_params.insert("client_id", &config.client_id);

        let poll_response = client
            .post(&config.token_url)
            .form(&poll_params)
            .send()
            .await
            .map_err(|e| LlmError::OAuthFailed {
                message: format!("Token poll request failed: {}", e),
            })?;

        let poll_status = poll_response.status();
        let poll_body = poll_response
            .text()
            .await
            .map_err(|e| LlmError::OAuthFailed {
                message: format!("Failed to read token poll response: {}", e),
            })?;

        if poll_status.is_success() {
            return parse_token_response(&poll_body);
        }

        // Check for "authorization_pending" or "slow_down" errors.
        if let Ok(err_json) = serde_json::from_str::<serde_json::Value>(&poll_body) {
            let error = err_json["error"].as_str().unwrap_or("");
            match error {
                "authorization_pending" => {
                    debug!("Device code flow: authorization pending, polling again...");
                    continue;
                }
                "slow_down" => {
                    debug!("Device code flow: slow down requested");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
                "expired_token" => {
                    return Err(LlmError::OAuthFailed {
                        message: "Device code expired. Please try again.".to_string(),
                    });
                }
                "access_denied" => {
                    return Err(LlmError::OAuthFailed {
                        message: "Authorization was denied by the user.".to_string(),
                    });
                }
                _ => {
                    return Err(LlmError::OAuthFailed {
                        message: format!("Token poll error: {}", poll_body),
                    });
                }
            }
        }

        // Non-JSON error response.
        return Err(LlmError::OAuthFailed {
            message: format!("Token poll failed (HTTP {}): {}", poll_status, poll_body),
        });
    }
}

// ── Token Refresh ───────────────────────────────────────────────────────────

/// Refresh an OAuth access token using a refresh token.
pub async fn refresh_token(
    config: &OAuthProviderConfig,
    refresh_token_str: &str,
) -> Result<OAuthToken, LlmError> {
    let client = reqwest::Client::new();

    let mut params = HashMap::new();
    params.insert("grant_type", "refresh_token");
    params.insert("refresh_token", refresh_token_str);
    params.insert("client_id", &config.client_id);

    debug!(provider = %config.provider_name, "Refreshing OAuth token");

    let response = client
        .post(&config.token_url)
        .form(&params)
        .send()
        .await
        .map_err(|e| LlmError::OAuthFailed {
            message: format!("Token refresh request failed: {}", e),
        })?;

    let status = response.status();
    let body_text = response.text().await.map_err(|e| LlmError::OAuthFailed {
        message: format!("Failed to read token refresh response: {}", e),
    })?;

    if !status.is_success() {
        return Err(LlmError::OAuthFailed {
            message: format!("Token refresh failed (HTTP {}): {}", status, body_text),
        });
    }

    let mut token = parse_token_response(&body_text)?;

    // Some providers don't return a new refresh_token; preserve the old one.
    if token.refresh_token.is_none() {
        token.refresh_token = Some(refresh_token_str.to_string());
    }

    Ok(token)
}

// ── Token Expiration ────────────────────────────────────────────────────────

/// Check whether an OAuth token has expired (with a 5-minute safety buffer).
pub fn is_token_expired(token: &OAuthToken) -> bool {
    match token.expires_at {
        Some(expires_at) => {
            let buffer = chrono::Duration::minutes(5);
            Utc::now() >= (expires_at - buffer)
        }
        // No expiration info — assume it's still valid.
        None => false,
    }
}

// ── Token Storage ───────────────────────────────────────────────────────────

/// Store an OAuth token in the credential store.
///
/// The token is serialized as JSON and stored under the key `oauth:{provider}`.
pub fn store_oauth_token(
    store: &dyn CredentialStore,
    provider: &str,
    token: &OAuthToken,
) -> Result<(), LlmError> {
    let key = format!("oauth:{}", provider);
    let json = serde_json::to_string(token).map_err(|e| LlmError::OAuthFailed {
        message: format!("Failed to serialize OAuth token: {}", e),
    })?;
    store
        .store_key(&key, &json)
        .map_err(|e| LlmError::OAuthFailed {
            message: format!("Failed to store OAuth token: {}", e),
        })
}

/// Load an OAuth token from the credential store.
pub fn load_oauth_token(
    store: &dyn CredentialStore,
    provider: &str,
) -> Result<OAuthToken, LlmError> {
    let key = format!("oauth:{}", provider);
    let json = store.get_key(&key).map_err(|e| match e {
        CredentialError::NotFound { .. } => LlmError::OAuthFailed {
            message: format!("No OAuth token found for provider '{}'", provider),
        },
        other => LlmError::OAuthFailed {
            message: format!("Failed to load OAuth token: {}", other),
        },
    })?;
    serde_json::from_str(&json).map_err(|e| LlmError::OAuthFailed {
        message: format!("Failed to deserialize OAuth token: {}", e),
    })
}

/// Delete an OAuth token from the credential store.
pub fn delete_oauth_token(store: &dyn CredentialStore, provider: &str) -> Result<(), LlmError> {
    let key = format!("oauth:{}", provider);
    store.delete_key(&key).map_err(|e| LlmError::OAuthFailed {
        message: format!("Failed to delete OAuth token: {}", e),
    })
}

/// Check whether an OAuth token exists in the credential store.
pub fn has_oauth_token(store: &dyn CredentialStore, provider: &str) -> bool {
    let key = format!("oauth:{}", provider);
    store.has_key(&key)
}

// ── Provider Configs ────────────────────────────────────────────────────────

/// OAuth configuration for OpenAI.
///
/// Uses the Codex public client ID. Supports both browser and device code flows.
pub fn openai_oauth_config() -> OAuthProviderConfig {
    OAuthProviderConfig {
        provider_name: "openai".to_string(),
        client_id: "app_EMoamEEZ73f0CkXaXp7hrann".to_string(),
        client_secret: None, // public PKCE client
        authorization_url: "https://auth.openai.com/oauth/authorize".to_string(),
        token_url: "https://auth.openai.com/oauth/token".to_string(),
        scopes: vec![
            "openid".to_string(),
            "profile".to_string(),
            "email".to_string(),
            "offline_access".to_string(),
        ],
        audience: None,
        supports_device_code: true,
        device_code_url: Some("https://auth.openai.com/oauth/device/code".to_string()),
        extra_auth_params: vec![
            ("id_token_add_organizations".to_string(), "true".to_string()),
            ("codex_cli_simplified_flow".to_string(), "true".to_string()),
            ("originator".to_string(), "codex_cli_rs".to_string()),
        ],
    }
}

/// OAuth configuration for Google (Gemini).
///
/// Requires a GCP OAuth client ID configured via the `GOOGLE_OAUTH_CLIENT_ID`
/// environment variable. Users must create an OAuth 2.0 client in the GCP Console
/// (application type: Desktop) with the Generative Language API scope enabled.
pub fn google_oauth_config() -> Option<OAuthProviderConfig> {
    let client_id = std::env::var("GOOGLE_OAUTH_CLIENT_ID").ok()?;
    let client_secret = std::env::var("GOOGLE_OAUTH_CLIENT_SECRET").ok();
    Some(OAuthProviderConfig {
        provider_name: "google".to_string(),
        client_id,
        client_secret,
        authorization_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
        token_url: "https://oauth2.googleapis.com/token".to_string(),
        scopes: vec!["https://www.googleapis.com/auth/generative-language".to_string()],
        audience: None,
        supports_device_code: false,
        device_code_url: None,
        extra_auth_params: vec![],
    })
}

/// OAuth configuration for Anthropic.
///
/// Currently returns `None` because Anthropic has blocked third-party tools from
/// using their OAuth endpoints as of January 2026. When/if Anthropic opens a
/// third-party OAuth program, this function will be updated to return a config.
pub fn anthropic_oauth_config() -> Option<OAuthProviderConfig> {
    // Anthropic OAuth is not available for third-party tools.
    // The infrastructure is ready; only a valid client_id is needed.
    None
}

// ── Channel OAuth Configs ──────────────────────────────────────────────────

/// OAuth configuration for Slack.
///
/// Uses Slack's OAuth 2.0 V2 flow with bot scopes for channel messaging,
/// history reading, and user info. Requires a Slack App client ID.
pub fn slack_oauth_config(client_id: &str, client_secret: Option<String>) -> OAuthProviderConfig {
    OAuthProviderConfig {
        provider_name: "slack".to_string(),
        client_id: client_id.to_string(),
        client_secret,
        authorization_url: "https://slack.com/oauth/v2/authorize".to_string(),
        token_url: "https://slack.com/api/oauth.v2.access".to_string(),
        scopes: vec![
            "chat:write".to_string(),
            "channels:history".to_string(),
            "channels:read".to_string(),
            "users:read".to_string(),
        ],
        audience: None,
        supports_device_code: false,
        device_code_url: None,
        extra_auth_params: vec![],
    }
}

/// OAuth configuration for Discord.
///
/// Uses Discord's OAuth 2.0 flow with bot scope for messaging and reading.
/// Requires a Discord Application client ID.
pub fn discord_oauth_config(client_id: &str, client_secret: Option<String>) -> OAuthProviderConfig {
    OAuthProviderConfig {
        provider_name: "discord".to_string(),
        client_id: client_id.to_string(),
        client_secret,
        authorization_url: "https://discord.com/api/oauth2/authorize".to_string(),
        token_url: "https://discord.com/api/oauth2/token".to_string(),
        scopes: vec!["bot".to_string(), "messages.read".to_string()],
        audience: None,
        supports_device_code: false,
        device_code_url: None,
        extra_auth_params: vec![],
    }
}

/// OAuth configuration for Microsoft Teams via Azure AD.
///
/// Uses Azure AD's OAuth 2.0 flow with Microsoft Graph scopes.
/// The `tenant_id` can be "common" for multi-tenant apps or a specific
/// Azure AD tenant ID. Teams bots typically use the client credentials
/// grant (server-to-server), but this config also supports the authorization
/// code flow for user-delegated access.
pub fn teams_oauth_config(
    client_id: &str,
    tenant_id: &str,
    client_secret: Option<String>,
) -> OAuthProviderConfig {
    OAuthProviderConfig {
        provider_name: "teams".to_string(),
        client_id: client_id.to_string(),
        client_secret,
        authorization_url: format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/authorize",
            tenant_id
        ),
        token_url: format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/token",
            tenant_id
        ),
        scopes: vec!["https://graph.microsoft.com/.default".to_string()],
        audience: None,
        supports_device_code: true,
        device_code_url: Some(format!(
            "https://login.microsoftonline.com/{}/oauth2/v2.0/devicecode",
            tenant_id
        )),
        extra_auth_params: vec![],
    }
}

/// OAuth configuration for WhatsApp via Meta Business Platform.
///
/// Uses Meta's OAuth 2.0 flow for WhatsApp Business API access.
/// Requires a Meta App ID as the client ID.
pub fn whatsapp_oauth_config(app_id: &str, app_secret: Option<String>) -> OAuthProviderConfig {
    OAuthProviderConfig {
        provider_name: "whatsapp".to_string(),
        client_id: app_id.to_string(),
        client_secret: app_secret,
        authorization_url: "https://www.facebook.com/v18.0/dialog/oauth".to_string(),
        token_url: "https://graph.facebook.com/v18.0/oauth/access_token".to_string(),
        scopes: vec![
            "whatsapp_business_messaging".to_string(),
            "whatsapp_business_management".to_string(),
        ],
        audience: None,
        supports_device_code: false,
        device_code_url: None,
        extra_auth_params: vec![],
    }
}

/// OAuth configuration for Gmail (IMAP/SMTP with XOAUTH2).
///
/// Reuses Google's OAuth 2.0 endpoints with the Gmail-specific scope for
/// full mailbox access via IMAP and SMTP XOAUTH2 SASL authentication.
/// Requires a GCP OAuth client ID.
pub fn gmail_oauth_config(client_id: &str, client_secret: Option<String>) -> OAuthProviderConfig {
    OAuthProviderConfig {
        provider_name: "gmail".to_string(),
        client_id: client_id.to_string(),
        client_secret,
        authorization_url: "https://accounts.google.com/o/oauth2/v2/auth".to_string(),
        token_url: "https://oauth2.googleapis.com/token".to_string(),
        scopes: vec!["https://mail.google.com/".to_string()],
        audience: None,
        supports_device_code: false,
        device_code_url: None,
        extra_auth_params: vec![
            ("access_type".to_string(), "offline".to_string()),
            ("prompt".to_string(), "consent".to_string()),
        ],
    }
}

// ── Client Credentials Flow ────────────────────────────────────────────────

/// Run the OAuth 2.0 Client Credentials flow (server-to-server).
///
/// This flow is used by services like Microsoft Teams bots that authenticate
/// as the application itself rather than a user. It requires both the client ID
/// (in the config) and a client secret.
///
/// Returns an `OAuthToken` with an access token and expiration.
pub async fn authorize_client_credentials_flow(
    config: &OAuthProviderConfig,
    client_secret: &str,
) -> Result<OAuthToken, LlmError> {
    let client = reqwest::Client::new();

    // Use the explicit parameter, falling back to config.client_secret if empty.
    let secret = if client_secret.is_empty() {
        config.client_secret.as_deref().unwrap_or("")
    } else {
        client_secret
    };

    let body = format!(
        "grant_type={}&client_id={}&client_secret={}&scope={}",
        urlencoding::encode("client_credentials"),
        urlencoding::encode(&config.client_id),
        urlencoding::encode(secret),
        urlencoding::encode(&config.scopes.join(" ")),
    );

    debug!(provider = %config.provider_name, "Requesting client credentials token");

    let response = client
        .post(&config.token_url)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .body(body)
        .send()
        .await
        .map_err(|e| LlmError::OAuthFailed {
            message: format!("Client credentials request failed: {}", e),
        })?;

    let status = response.status();
    let body_text = response.text().await.map_err(|e| LlmError::OAuthFailed {
        message: format!("Failed to read client credentials response: {}", e),
    })?;

    if !status.is_success() {
        return Err(LlmError::OAuthFailed {
            message: format!(
                "Client credentials token request failed (HTTP {}): {}",
                status, body_text
            ),
        });
    }

    parse_token_response(&body_text)
}

/// Build an XOAUTH2 SASL token string for IMAP/SMTP authentication.
///
/// Format: `user=<email>\x01auth=Bearer <token>\x01\x01`
/// This is used by Gmail and other providers that support XOAUTH2.
pub fn build_xoauth2_token(email: &str, access_token: &str) -> String {
    format!("user={}\x01auth=Bearer {}\x01\x01", email, access_token)
}

/// Base64-encode an XOAUTH2 token for SASL AUTH.
pub fn build_xoauth2_token_base64(email: &str, access_token: &str) -> String {
    use base64::Engine;
    let raw = build_xoauth2_token(email, access_token);
    base64::engine::general_purpose::STANDARD.encode(raw.as_bytes())
}

/// Look up the OAuth configuration for a provider by name.
///
/// Returns `None` if the provider does not support OAuth or if required
/// environment variables (e.g., `GOOGLE_OAUTH_CLIENT_ID`) are not set.
///
/// For channel providers (slack, discord, teams, whatsapp, gmail), the relevant
/// client ID / app ID environment variables must be set.
pub fn oauth_config_for_provider(provider: &str) -> Option<OAuthProviderConfig> {
    match provider {
        "openai" => Some(openai_oauth_config()),
        "gemini" | "google" => google_oauth_config(),
        "anthropic" => anthropic_oauth_config(),
        "slack" => {
            let client_id = std::env::var("SLACK_CLIENT_ID").ok()?;
            let client_secret = std::env::var("SLACK_CLIENT_SECRET").ok();
            Some(slack_oauth_config(&client_id, client_secret))
        }
        "discord" => {
            let client_id = std::env::var("DISCORD_CLIENT_ID").ok()?;
            let client_secret = std::env::var("DISCORD_CLIENT_SECRET").ok();
            Some(discord_oauth_config(&client_id, client_secret))
        }
        "teams" => {
            let client_id = std::env::var("TEAMS_CLIENT_ID").ok()?;
            let tenant_id =
                std::env::var("TEAMS_TENANT_ID").unwrap_or_else(|_| "common".to_string());
            let client_secret = std::env::var("TEAMS_CLIENT_SECRET").ok();
            Some(teams_oauth_config(&client_id, &tenant_id, client_secret))
        }
        "whatsapp" => {
            let app_id = std::env::var("WHATSAPP_APP_ID").ok()?;
            let app_secret = std::env::var("WHATSAPP_APP_SECRET").ok();
            Some(whatsapp_oauth_config(&app_id, app_secret))
        }
        "gmail" => {
            let client_id = std::env::var("GMAIL_OAUTH_CLIENT_ID")
                .or_else(|_| std::env::var("GOOGLE_OAUTH_CLIENT_ID"))
                .ok()?;
            let client_secret = std::env::var("GMAIL_OAUTH_CLIENT_SECRET")
                .or_else(|_| std::env::var("GOOGLE_OAUTH_CLIENT_SECRET"))
                .ok();
            Some(gmail_oauth_config(&client_id, client_secret))
        }
        _ => None,
    }
}

/// Build an OAuth configuration using directly-provided credentials.
///
/// Unlike [`oauth_config_for_provider`] which reads client credentials from
/// environment variables, this function accepts them as parameters. This is
/// used by the interactive `channel setup` wizard where the user enters
/// credentials at a prompt rather than setting env vars.
pub fn oauth_config_with_credentials(
    provider: &str,
    client_id: &str,
    client_secret: Option<&str>,
) -> Option<OAuthProviderConfig> {
    let secret = client_secret.map(String::from);
    match provider {
        "slack" => Some(slack_oauth_config(client_id, secret)),
        "discord" => Some(discord_oauth_config(client_id, secret)),
        "gmail" => Some(gmail_oauth_config(client_id, secret)),
        _ => None,
    }
}

/// Check whether a provider supports OAuth login.
pub fn provider_supports_oauth(provider: &str) -> bool {
    match provider {
        "openai" => true,
        "gemini" | "google" => std::env::var("GOOGLE_OAUTH_CLIENT_ID").is_ok(),
        "slack" => std::env::var("SLACK_CLIENT_ID").is_ok(),
        "discord" => std::env::var("DISCORD_CLIENT_ID").is_ok(),
        "teams" => std::env::var("TEAMS_CLIENT_ID").is_ok(),
        "whatsapp" => std::env::var("WHATSAPP_APP_ID").is_ok(),
        "gmail" => {
            std::env::var("GMAIL_OAUTH_CLIENT_ID").is_ok()
                || std::env::var("GOOGLE_OAUTH_CLIENT_ID").is_ok()
        }
        _ => false,
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::InMemoryCredentialStore;

    #[test]
    fn test_generate_pkce_pair() {
        let pair = generate_pkce_pair();
        assert_eq!(pair.verifier.len(), 43);
        assert!(!pair.challenge.is_empty());

        // Verify the challenge is a valid base64url-encoded SHA-256 hash.
        let decoded = URL_SAFE_NO_PAD.decode(&pair.challenge).unwrap();
        assert_eq!(decoded.len(), 32); // SHA-256 produces 32 bytes

        // Verify the challenge matches the verifier.
        let mut hasher = Sha256::new();
        hasher.update(pair.verifier.as_bytes());
        let expected = hasher.finalize();
        assert_eq!(decoded, expected.as_slice());
    }

    #[test]
    fn test_generate_pkce_pair_uniqueness() {
        let pair1 = generate_pkce_pair();
        let pair2 = generate_pkce_pair();
        assert_ne!(pair1.verifier, pair2.verifier);
        assert_ne!(pair1.challenge, pair2.challenge);
    }

    #[test]
    fn test_generate_state() {
        let state = generate_state();
        assert!(!state.is_empty());
        // base64url of 32 bytes = 43 characters
        assert_eq!(state.len(), 43);
    }

    #[test]
    fn test_generate_state_uniqueness() {
        let s1 = generate_state();
        let s2 = generate_state();
        assert_ne!(s1, s2);
    }

    #[test]
    fn test_parse_token_response_full() {
        let body = serde_json::json!({
            "access_token": "at-12345",
            "refresh_token": "rt-67890",
            "token_type": "Bearer",
            "expires_in": 3600,
            "scope": "openai.public"
        })
        .to_string();

        let token = parse_token_response(&body).unwrap();
        assert_eq!(token.access_token, "at-12345");
        assert_eq!(token.refresh_token, Some("rt-67890".to_string()));
        assert_eq!(token.token_type, "Bearer");
        assert!(token.expires_at.is_some());
        assert_eq!(token.scopes, vec!["openai.public"]);
    }

    #[test]
    fn test_parse_token_response_minimal() {
        let body = serde_json::json!({
            "access_token": "at-minimal"
        })
        .to_string();

        let token = parse_token_response(&body).unwrap();
        assert_eq!(token.access_token, "at-minimal");
        assert!(token.refresh_token.is_none());
        assert_eq!(token.token_type, "Bearer");
        assert!(token.expires_at.is_none());
        assert!(token.scopes.is_empty());
    }

    #[test]
    fn test_parse_token_response_missing_access_token() {
        let body = serde_json::json!({
            "token_type": "Bearer"
        })
        .to_string();

        let result = parse_token_response(&body);
        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::OAuthFailed { message } => {
                assert!(message.contains("access_token"));
            }
            other => panic!("Expected OAuthFailed, got {:?}", other),
        }
    }

    #[test]
    fn test_parse_token_response_invalid_json() {
        let result = parse_token_response("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_is_token_expired_future() {
        let token = OAuthToken {
            access_token: "test".to_string(),
            refresh_token: None,
            id_token: None,
            expires_at: Some(Utc::now() + chrono::Duration::hours(1)),
            token_type: "Bearer".to_string(),
            scopes: vec![],
        };
        assert!(!is_token_expired(&token));
    }

    #[test]
    fn test_is_token_expired_past() {
        let token = OAuthToken {
            access_token: "test".to_string(),
            refresh_token: None,
            id_token: None,
            expires_at: Some(Utc::now() - chrono::Duration::hours(1)),
            token_type: "Bearer".to_string(),
            scopes: vec![],
        };
        assert!(is_token_expired(&token));
    }

    #[test]
    fn test_is_token_expired_within_buffer() {
        // Token expires in 3 minutes — within the 5-minute buffer.
        let token = OAuthToken {
            access_token: "test".to_string(),
            refresh_token: None,
            id_token: None,
            expires_at: Some(Utc::now() + chrono::Duration::minutes(3)),
            token_type: "Bearer".to_string(),
            scopes: vec![],
        };
        assert!(is_token_expired(&token));
    }

    #[test]
    fn test_is_token_expired_no_expiry() {
        let token = OAuthToken {
            access_token: "test".to_string(),
            refresh_token: None,
            id_token: None,
            expires_at: None,
            token_type: "Bearer".to_string(),
            scopes: vec![],
        };
        assert!(!is_token_expired(&token));
    }

    #[test]
    fn test_store_and_load_oauth_token() {
        let store = InMemoryCredentialStore::new();
        let token = OAuthToken {
            access_token: "at-test-store".to_string(),
            refresh_token: Some("rt-test-store".to_string()),
            id_token: None,
            expires_at: None,
            token_type: "Bearer".to_string(),
            scopes: vec!["openai.public".to_string()],
        };

        store_oauth_token(&store, "openai", &token).unwrap();
        let loaded = load_oauth_token(&store, "openai").unwrap();
        assert_eq!(loaded.access_token, "at-test-store");
        assert_eq!(loaded.refresh_token, Some("rt-test-store".to_string()));
        assert_eq!(loaded.scopes, vec!["openai.public"]);
    }

    #[test]
    fn test_load_oauth_token_not_found() {
        let store = InMemoryCredentialStore::new();
        let result = load_oauth_token(&store, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_oauth_token() {
        let store = InMemoryCredentialStore::new();
        let token = OAuthToken {
            access_token: "at-delete".to_string(),
            refresh_token: None,
            id_token: None,
            expires_at: None,
            token_type: "Bearer".to_string(),
            scopes: vec![],
        };

        store_oauth_token(&store, "openai", &token).unwrap();
        assert!(has_oauth_token(&store, "openai"));

        delete_oauth_token(&store, "openai").unwrap();
        assert!(!has_oauth_token(&store, "openai"));
    }

    #[test]
    fn test_has_oauth_token() {
        let store = InMemoryCredentialStore::new();
        assert!(!has_oauth_token(&store, "openai"));

        let token = OAuthToken {
            access_token: "at-has".to_string(),
            refresh_token: None,
            id_token: None,
            expires_at: None,
            token_type: "Bearer".to_string(),
            scopes: vec![],
        };
        store_oauth_token(&store, "openai", &token).unwrap();
        assert!(has_oauth_token(&store, "openai"));
    }

    #[test]
    fn test_openai_oauth_config() {
        let config = openai_oauth_config();
        assert_eq!(config.provider_name, "openai");
        assert_eq!(config.client_id, "app_EMoamEEZ73f0CkXaXp7hrann");
        assert!(config.authorization_url.contains("auth.openai.com"));
        assert!(config.token_url.contains("auth.openai.com"));
        assert!(config.supports_device_code);
        assert!(config.device_code_url.is_some());
        assert_eq!(
            config.scopes,
            vec!["openid", "profile", "email", "offline_access"]
        );
        assert_eq!(config.audience, None);
        assert_eq!(config.extra_auth_params.len(), 3);
    }

    #[test]
    fn test_anthropic_oauth_config_returns_none() {
        assert!(anthropic_oauth_config().is_none());
    }

    #[test]
    fn test_oauth_config_for_provider() {
        assert!(oauth_config_for_provider("openai").is_some());
        assert!(oauth_config_for_provider("anthropic").is_none());
        assert!(oauth_config_for_provider("unknown").is_none());
    }

    #[test]
    fn test_provider_supports_oauth() {
        assert!(provider_supports_oauth("openai"));
        assert!(!provider_supports_oauth("anthropic"));
        assert!(!provider_supports_oauth("unknown"));
    }

    #[test]
    fn test_auth_method_serde() {
        let json = serde_json::to_string(&AuthMethod::OAuth).unwrap();
        assert_eq!(json, "\"oauth\"");
        let method: AuthMethod = serde_json::from_str("\"api_key\"").unwrap();
        assert_eq!(method, AuthMethod::ApiKey);
    }

    #[test]
    fn test_auth_method_default() {
        assert_eq!(AuthMethod::default(), AuthMethod::ApiKey);
    }

    #[test]
    fn test_auth_method_display() {
        assert_eq!(AuthMethod::ApiKey.to_string(), "api_key");
        assert_eq!(AuthMethod::OAuth.to_string(), "oauth");
    }

    #[test]
    fn test_oauth_token_serde_roundtrip() {
        let token = OAuthToken {
            access_token: "at-roundtrip".to_string(),
            refresh_token: Some("rt-roundtrip".to_string()),
            id_token: None,
            expires_at: Some(Utc::now()),
            token_type: "Bearer".to_string(),
            scopes: vec!["scope1".to_string(), "scope2".to_string()],
        };
        let json = serde_json::to_string(&token).unwrap();
        let parsed: OAuthToken = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.access_token, token.access_token);
        assert_eq!(parsed.refresh_token, token.refresh_token);
        assert_eq!(parsed.scopes.len(), 2);
    }

    #[tokio::test]
    async fn test_callback_server_http_receives_code() {
        let (port, rx) = start_callback_server(false).await.unwrap();
        assert_eq!(port, OAUTH_CALLBACK_PORT);

        // Simulate the OAuth callback using plain HTTP.
        let client = reqwest::Client::new();
        let url = format!(
            "http://127.0.0.1:{}/auth/callback?code=test-http&state=test-state-http",
            port
        );
        let response = client.get(&url).send().await.unwrap();
        assert!(response.status().is_success());

        let callback = rx.await.unwrap();
        assert_eq!(callback.code, "test-http");
        assert_eq!(callback.state, "test-state-http");
    }

    #[tokio::test]
    async fn test_tls_config_loading() {
        // Verify we can load/generate a TLS config without errors.
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let config = load_tls_config().await;
        assert!(config.is_ok(), "TLS config loading should succeed");
    }

    // ── Channel OAuth Config Tests ─────────────────────────────────────────

    #[test]
    fn test_slack_oauth_config() {
        let config = slack_oauth_config("slack-client-123", Some("slack-secret".into()));
        assert_eq!(config.provider_name, "slack");
        assert_eq!(config.client_id, "slack-client-123");
        assert!(config
            .authorization_url
            .contains("slack.com/oauth/v2/authorize"));
        assert!(config.token_url.contains("slack.com/api/oauth.v2.access"));
        assert!(config.scopes.contains(&"chat:write".to_string()));
        assert!(config.scopes.contains(&"channels:history".to_string()));
        assert!(config.scopes.contains(&"channels:read".to_string()));
        assert!(config.scopes.contains(&"users:read".to_string()));
        assert!(!config.supports_device_code);
    }

    #[test]
    fn test_discord_oauth_config() {
        let config = discord_oauth_config("discord-client-456", Some("discord-secret".into()));
        assert_eq!(config.provider_name, "discord");
        assert_eq!(config.client_id, "discord-client-456");
        assert!(config
            .authorization_url
            .contains("discord.com/api/oauth2/authorize"));
        assert!(config.token_url.contains("discord.com/api/oauth2/token"));
        assert!(config.scopes.contains(&"bot".to_string()));
        assert!(config.scopes.contains(&"messages.read".to_string()));
        assert!(!config.supports_device_code);
    }

    #[test]
    fn test_teams_oauth_config() {
        let config = teams_oauth_config(
            "teams-client-789",
            "my-tenant-id",
            Some("teams-secret".into()),
        );
        assert_eq!(config.provider_name, "teams");
        assert_eq!(config.client_id, "teams-client-789");
        assert!(config
            .authorization_url
            .contains("login.microsoftonline.com/my-tenant-id"));
        assert!(config
            .token_url
            .contains("login.microsoftonline.com/my-tenant-id"));
        assert!(config
            .scopes
            .contains(&"https://graph.microsoft.com/.default".to_string()));
        assert!(config.supports_device_code);
        assert!(config
            .device_code_url
            .as_ref()
            .unwrap()
            .contains("my-tenant-id"));
    }

    #[test]
    fn test_teams_oauth_config_common_tenant() {
        let config = teams_oauth_config("teams-client", "common", None);
        assert!(config
            .authorization_url
            .contains("common/oauth2/v2.0/authorize"));
        assert!(config.token_url.contains("common/oauth2/v2.0/token"));
    }

    #[test]
    fn test_whatsapp_oauth_config() {
        let config = whatsapp_oauth_config("meta-app-123", Some("meta-secret".into()));
        assert_eq!(config.provider_name, "whatsapp");
        assert_eq!(config.client_id, "meta-app-123");
        assert!(config
            .authorization_url
            .contains("facebook.com/v18.0/dialog/oauth"));
        assert!(config
            .token_url
            .contains("graph.facebook.com/v18.0/oauth/access_token"));
        assert!(config
            .scopes
            .contains(&"whatsapp_business_messaging".to_string()));
        assert!(config
            .scopes
            .contains(&"whatsapp_business_management".to_string()));
        assert!(!config.supports_device_code);
    }

    #[test]
    fn test_gmail_oauth_config() {
        let config = gmail_oauth_config("gmail-client-id", Some("gmail-secret".into()));
        assert_eq!(config.provider_name, "gmail");
        assert_eq!(config.client_id, "gmail-client-id");
        assert!(config.authorization_url.contains("accounts.google.com"));
        assert!(config.token_url.contains("oauth2.googleapis.com"));
        assert!(config
            .scopes
            .contains(&"https://mail.google.com/".to_string()));
        // Gmail config should request offline access
        assert!(config
            .extra_auth_params
            .iter()
            .any(|(k, v)| k == "access_type" && v == "offline"));
    }

    #[test]
    fn test_xoauth2_token_format() {
        let token = build_xoauth2_token("user@gmail.com", "ya29.access-token");
        assert_eq!(
            token,
            "user=user@gmail.com\x01auth=Bearer ya29.access-token\x01\x01"
        );
    }

    #[test]
    fn test_xoauth2_token_base64() {
        let b64 = build_xoauth2_token_base64("user@gmail.com", "token123");
        // Should be valid base64
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&b64)
            .unwrap();
        let decoded_str = String::from_utf8(decoded).unwrap();
        assert!(decoded_str.starts_with("user=user@gmail.com\x01"));
        assert!(decoded_str.contains("auth=Bearer token123"));
    }

    #[test]
    fn test_oauth_config_for_channel_providers_without_env() {
        // Without env vars set, channel providers should return None
        // (unless env vars happen to be set in CI)
        let _ = oauth_config_for_provider("slack");
        let _ = oauth_config_for_provider("discord");
        let _ = oauth_config_for_provider("teams");
        let _ = oauth_config_for_provider("whatsapp");
        let _ = oauth_config_for_provider("gmail");
        // Just verifying they don't panic
    }

    #[test]
    fn test_store_and_load_channel_oauth_token() {
        let store = InMemoryCredentialStore::new();
        let token = OAuthToken {
            access_token: "xoxb-slack-token".to_string(),
            refresh_token: Some("xoxr-refresh".to_string()),
            id_token: None,
            expires_at: None,
            token_type: "Bearer".to_string(),
            scopes: vec!["chat:write".to_string(), "channels:history".to_string()],
        };

        store_oauth_token(&store, "slack", &token).unwrap();
        let loaded = load_oauth_token(&store, "slack").unwrap();
        assert_eq!(loaded.access_token, "xoxb-slack-token");
        assert_eq!(loaded.scopes.len(), 2);

        // Store a second provider token
        let teams_token = OAuthToken {
            access_token: "eyJ-teams-token".to_string(),
            refresh_token: None,
            id_token: None,
            expires_at: None,
            token_type: "Bearer".to_string(),
            scopes: vec!["https://graph.microsoft.com/.default".to_string()],
        };
        store_oauth_token(&store, "teams", &teams_token).unwrap();
        let loaded_teams = load_oauth_token(&store, "teams").unwrap();
        assert_eq!(loaded_teams.access_token, "eyJ-teams-token");

        // Original slack token should still be there
        let loaded_slack = load_oauth_token(&store, "slack").unwrap();
        assert_eq!(loaded_slack.access_token, "xoxb-slack-token");
    }
}

//! Model listing and metadata for LLM providers.
//!
//! Provides functions to fetch available models from provider APIs (OpenAI-compatible)
//! or return hardcoded known models (Anthropic), along with filtering utilities.

use crate::error::LlmError;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;

/// Metadata about a single LLM model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    /// The model identifier (e.g., "gpt-4o", "claude-sonnet-4-20250514").
    pub id: String,
    /// Human-readable model name.
    pub name: String,
    /// Context window size in tokens, if known.
    pub context_window: Option<usize>,
    /// Whether this model supports chat/completion requests.
    pub is_chat_model: bool,
}

/// Parse an OpenAI `/models` API response into a list of `ModelInfo`.
///
/// Expects a JSON body with a `"data"` array of model objects, each with at least an `"id"` field.
pub fn parse_openai_models_response(body: &Value) -> Result<Vec<ModelInfo>, LlmError> {
    let data =
        body.get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| LlmError::ResponseParse {
                message: "Missing 'data' array in models response".to_string(),
            })?;

    let mut models: Vec<ModelInfo> = data
        .iter()
        .filter_map(|m| {
            let id = m.get("id")?.as_str()?.to_string();
            Some(ModelInfo {
                name: id.clone(),
                id,
                context_window: None,
                is_chat_model: true,
            })
        })
        .collect();

    models.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(models)
}

/// Filter a list of models to include only chat/completion models.
///
/// Excludes embedding, whisper, tts, dall-e, moderation, and legacy text-* models.
pub fn filter_chat_models(models: Vec<ModelInfo>) -> Vec<ModelInfo> {
    models
        .into_iter()
        .filter(|m| {
            let id = m.id.to_lowercase();
            !id.contains("embedding")
                && !id.contains("whisper")
                && !id.contains("tts")
                && !id.contains("dall-e")
                && !id.contains("moderation")
                && !id.starts_with("text-")
        })
        .collect()
}

/// Return a hardcoded list of known Anthropic Claude models.
///
/// Anthropic does not provide a public `/models` API endpoint, so this list
/// is maintained manually and should be updated when new models are released.
pub fn anthropic_known_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "claude-opus-4-20250514".to_string(),
            name: "Claude Opus 4".to_string(),
            context_window: Some(200_000),
            is_chat_model: true,
        },
        ModelInfo {
            id: "claude-sonnet-4-20250514".to_string(),
            name: "Claude Sonnet 4".to_string(),
            context_window: Some(200_000),
            is_chat_model: true,
        },
        ModelInfo {
            id: "claude-3-5-sonnet-20241022".to_string(),
            name: "Claude 3.5 Sonnet".to_string(),
            context_window: Some(200_000),
            is_chat_model: true,
        },
        ModelInfo {
            id: "claude-3-5-haiku-20241022".to_string(),
            name: "Claude 3.5 Haiku".to_string(),
            context_window: Some(200_000),
            is_chat_model: true,
        },
    ]
}

/// Return a hardcoded list of known Google Gemini models.
///
/// Google does not provide a simple public `/models` listing suitable for
/// our setup wizard, so this list is maintained manually.
pub fn gemini_known_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "gemini-2.0-flash".to_string(),
            name: "Gemini 2.0 Flash".to_string(),
            context_window: Some(1_048_576),
            is_chat_model: true,
        },
        ModelInfo {
            id: "gemini-2.0-flash-lite".to_string(),
            name: "Gemini 2.0 Flash Lite".to_string(),
            context_window: Some(1_048_576),
            is_chat_model: true,
        },
        ModelInfo {
            id: "gemini-1.5-pro".to_string(),
            name: "Gemini 1.5 Pro".to_string(),
            context_window: Some(2_097_152),
            is_chat_model: true,
        },
        ModelInfo {
            id: "gemini-1.5-flash".to_string(),
            name: "Gemini 1.5 Flash".to_string(),
            context_window: Some(1_048_576),
            is_chat_model: true,
        },
    ]
}

/// Fetch available models from an OpenAI-compatible `/models` endpoint.
///
/// Sends a GET request to `{base_url}/models` with the provided API key.
/// Returns filtered chat models sorted by ID.
pub async fn fetch_openai_models(
    api_key: &str,
    base_url: Option<&str>,
) -> Result<Vec<ModelInfo>, LlmError> {
    let base = base_url.unwrap_or("https://api.openai.com/v1");
    let url = format!("{}/models", base);

    debug!(url = %url, "Fetching models from OpenAI-compatible endpoint");

    let client = Client::new();
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {}", api_key))
        .send()
        .await
        .map_err(|e| LlmError::ApiRequest {
            message: format!("Failed to fetch models: {}", e),
        })?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        return Err(match status.as_u16() {
            401 => LlmError::AuthFailed {
                provider: "OpenAI-compatible".to_string(),
            },
            429 => LlmError::RateLimited {
                retry_after_secs: 5,
            },
            _ => LlmError::ApiRequest {
                message: format!("HTTP {} fetching models: {}", status, body_text),
            },
        });
    }

    let body: Value = response.json().await.map_err(|e| LlmError::ResponseParse {
        message: format!("Invalid JSON in models response: {}", e),
    })?;

    let models = parse_openai_models_response(&body)?;
    Ok(filter_chat_models(models))
}

/// List available models for the given provider.
///
/// - For `"anthropic"`: returns a hardcoded list of known Claude models.
/// - For everything else: fetches from the OpenAI-compatible `/models` endpoint.
pub async fn list_models(
    provider: &str,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<Vec<ModelInfo>, LlmError> {
    match provider {
        "anthropic" => Ok(anthropic_known_models()),
        "gemini" => Ok(gemini_known_models()),
        _ => fetch_openai_models(api_key, base_url).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_openai_models_response() {
        let body = serde_json::json!({
            "data": [
                {"id": "gpt-4o", "object": "model", "owned_by": "openai"},
                {"id": "gpt-4o-mini", "object": "model", "owned_by": "openai"},
                {"id": "text-embedding-3-small", "object": "model", "owned_by": "openai"},
            ]
        });
        let models = parse_openai_models_response(&body).unwrap();
        assert_eq!(models.len(), 3);
        assert!(models.iter().any(|m| m.id == "gpt-4o"));
        assert!(models.iter().any(|m| m.id == "gpt-4o-mini"));
        assert!(models.iter().any(|m| m.id == "text-embedding-3-small"));
    }

    #[test]
    fn test_parse_empty_models_response() {
        let body = serde_json::json!({"data": []});
        let models = parse_openai_models_response(&body).unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn test_parse_missing_data_field() {
        let body = serde_json::json!({"error": "bad request"});
        let result = parse_openai_models_response(&body);
        assert!(result.is_err());
        match result.unwrap_err() {
            LlmError::ResponseParse { message } => {
                assert!(message.contains("data"));
            }
            other => panic!("Expected ResponseParse, got {:?}", other),
        }
    }

    #[test]
    fn test_anthropic_known_models_list() {
        let models = anthropic_known_models();
        assert!(models.len() >= 3);
        assert!(models.iter().all(|m| m.is_chat_model));
        assert!(models.iter().all(|m| m.context_window.is_some()));
        assert!(models.iter().any(|m| m.id.contains("sonnet")));
        assert!(models.iter().any(|m| m.id.contains("opus")));
        assert!(models.iter().any(|m| m.id.contains("haiku")));
    }

    #[test]
    fn test_gemini_known_models_list() {
        let models = gemini_known_models();
        assert!(models.len() >= 3);
        assert!(models.iter().all(|m| m.is_chat_model));
        assert!(models.iter().all(|m| m.context_window.is_some()));
        assert!(models.iter().any(|m| m.id.contains("flash")));
        assert!(models.iter().any(|m| m.id.contains("pro")));
    }

    #[test]
    fn test_model_info_fields() {
        let model = ModelInfo {
            id: "gpt-4o".to_string(),
            name: "GPT-4o".to_string(),
            context_window: Some(128_000),
            is_chat_model: true,
        };
        assert_eq!(model.id, "gpt-4o");
        assert_eq!(model.name, "GPT-4o");
        assert_eq!(model.context_window, Some(128_000));
        assert!(model.is_chat_model);
    }

    #[test]
    fn test_filter_chat_models() {
        let models = vec![
            ModelInfo {
                id: "gpt-4o".into(),
                name: "GPT-4o".into(),
                context_window: None,
                is_chat_model: true,
            },
            ModelInfo {
                id: "text-embedding-3-small".into(),
                name: "Embedding".into(),
                context_window: None,
                is_chat_model: true,
            },
            ModelInfo {
                id: "whisper-1".into(),
                name: "Whisper".into(),
                context_window: None,
                is_chat_model: true,
            },
            ModelInfo {
                id: "dall-e-3".into(),
                name: "DALL-E 3".into(),
                context_window: None,
                is_chat_model: true,
            },
            ModelInfo {
                id: "tts-1".into(),
                name: "TTS".into(),
                context_window: None,
                is_chat_model: true,
            },
            ModelInfo {
                id: "gpt-4o-mini".into(),
                name: "GPT-4o Mini".into(),
                context_window: None,
                is_chat_model: true,
            },
            ModelInfo {
                id: "text-moderation-latest".into(),
                name: "Moderation".into(),
                context_window: None,
                is_chat_model: true,
            },
        ];
        let filtered = filter_chat_models(models);
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().any(|m| m.id == "gpt-4o"));
        assert!(filtered.iter().any(|m| m.id == "gpt-4o-mini"));
    }
}

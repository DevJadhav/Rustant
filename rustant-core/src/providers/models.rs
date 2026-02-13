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

/// Return a hardcoded fallback list of known Anthropic Claude models.
///
/// Used only when the API call to list models fails.
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

/// Fetch available models from the Anthropic API.
///
/// Calls `GET /v1/models` with `x-api-key` and `anthropic-version` headers.
/// Falls back to `anthropic_known_models()` on failure.
pub async fn fetch_anthropic_models(api_key: &str) -> Result<Vec<ModelInfo>, LlmError> {
    let url = "https://api.anthropic.com/v1/models?limit=1000";

    debug!("Fetching models from Anthropic API");

    let client = Client::new();
    let response = client
        .get(url)
        .header("x-api-key", api_key)
        .header("anthropic-version", "2023-06-01")
        .send()
        .await
        .map_err(|e| LlmError::ApiRequest {
            message: format!("Failed to fetch Anthropic models: {}", e),
        })?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        return Err(match status.as_u16() {
            401 | 403 => LlmError::AuthFailed {
                provider: "Anthropic".to_string(),
            },
            429 => LlmError::RateLimited {
                retry_after_secs: 5,
            },
            _ => LlmError::ApiRequest {
                message: format!("HTTP {} fetching Anthropic models: {}", status, body_text),
            },
        });
    }

    let body: Value = response.json().await.map_err(|e| LlmError::ResponseParse {
        message: format!("Invalid JSON in Anthropic models response: {}", e),
    })?;

    parse_anthropic_models_response(&body)
}

/// Parse an Anthropic `/v1/models` API response into a list of `ModelInfo`.
///
/// Response format: `{"data": [{"id": "...", "display_name": "...", "created_at": "...", "type": "model"}]}`
/// More recently released models are listed first by the API.
pub fn parse_anthropic_models_response(body: &Value) -> Result<Vec<ModelInfo>, LlmError> {
    let data =
        body.get("data")
            .and_then(|d| d.as_array())
            .ok_or_else(|| LlmError::ResponseParse {
                message: "Missing 'data' array in Anthropic models response".to_string(),
            })?;

    let models: Vec<ModelInfo> = data
        .iter()
        .filter_map(|m| {
            let id = m.get("id")?.as_str()?.to_string();

            let display_name = m
                .get("display_name")
                .and_then(|d| d.as_str())
                .unwrap_or(&id)
                .to_string();

            // All Anthropic models listed via the API are chat models with 200k context
            // The API doesn't expose context_window, so use known defaults
            let context_window = Some(200_000);

            Some(ModelInfo {
                name: display_name,
                id,
                context_window,
                is_chat_model: true,
            })
        })
        .collect();

    // API returns newest first already, so no additional sorting needed
    Ok(models)
}

/// Return a hardcoded fallback list of known Google Gemini models.
///
/// Used only when the API call to list models fails.
pub fn gemini_known_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "gemini-2.5-pro".to_string(),
            name: "Gemini 2.5 Pro".to_string(),
            context_window: Some(1_048_576),
            is_chat_model: true,
        },
        ModelInfo {
            id: "gemini-2.5-flash".to_string(),
            name: "Gemini 2.5 Flash".to_string(),
            context_window: Some(1_048_576),
            is_chat_model: true,
        },
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

/// Fetch available models from the Google Gemini API.
///
/// Calls `GET /v1beta/models?key={api_key}&pageSize=1000` and filters to
/// models that support `generateContent` (i.e. chat/completion models).
/// Falls back to `gemini_known_models()` on failure.
pub async fn fetch_gemini_models(api_key: &str) -> Result<Vec<ModelInfo>, LlmError> {
    let base_url = "https://generativelanguage.googleapis.com/v1beta";
    let url = format!("{}/models?key={}&pageSize=1000", base_url, api_key);

    debug!(
        url = "GET /v1beta/models",
        "Fetching models from Gemini API"
    );

    let client = Client::new();
    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| LlmError::ApiRequest {
            message: format!("Failed to fetch Gemini models: {}", e),
        })?;

    let status = response.status();
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        return Err(match status.as_u16() {
            401 | 403 => LlmError::AuthFailed {
                provider: "Gemini".to_string(),
            },
            429 => LlmError::RateLimited {
                retry_after_secs: 5,
            },
            _ => LlmError::ApiRequest {
                message: format!("HTTP {} fetching Gemini models: {}", status, body_text),
            },
        });
    }

    let body: Value = response.json().await.map_err(|e| LlmError::ResponseParse {
        message: format!("Invalid JSON in Gemini models response: {}", e),
    })?;

    parse_gemini_models_response(&body)
}

/// Parse a Gemini `/v1beta/models` API response into a list of `ModelInfo`.
///
/// Filters to models that support `generateContent` (chat/completion models)
/// and excludes embedding, AQA, and legacy models.
pub fn parse_gemini_models_response(body: &Value) -> Result<Vec<ModelInfo>, LlmError> {
    let models_array = body
        .get("models")
        .and_then(|m| m.as_array())
        .ok_or_else(|| LlmError::ResponseParse {
            message: "Missing 'models' array in Gemini models response".to_string(),
        })?;

    let mut models: Vec<ModelInfo> = models_array
        .iter()
        .filter_map(|m| {
            // "name" is "models/gemini-2.0-flash" — strip the "models/" prefix
            let full_name = m.get("name")?.as_str()?;
            let id = full_name.strip_prefix("models/").unwrap_or(full_name);

            let display_name = m
                .get("displayName")
                .and_then(|d| d.as_str())
                .unwrap_or(id)
                .to_string();

            let input_limit = m
                .get("inputTokenLimit")
                .and_then(|v| v.as_u64())
                .map(|v| v as usize);

            // Only include models that support generateContent (chat models)
            let supported_methods = m
                .get("supportedGenerationMethods")
                .and_then(|v| v.as_array());
            let supports_generate = supported_methods
                .map(|methods| {
                    methods
                        .iter()
                        .any(|m| m.as_str() == Some("generateContent"))
                })
                .unwrap_or(false);

            if !supports_generate {
                return None;
            }

            // Skip embedding, AQA, and other non-chat models
            let id_lower = id.to_lowercase();
            if id_lower.contains("embedding")
                || id_lower.contains("aqa")
                || id_lower.contains("imagen")
                || id_lower.contains("veo")
                || id_lower.contains("lyria")
            {
                return None;
            }

            Some(ModelInfo {
                id: id.to_string(),
                name: display_name,
                context_window: input_limit,
                is_chat_model: true,
            })
        })
        .collect();

    // Sort: newest/most capable models first (by version descending, then name)
    models.sort_by(|a, b| b.id.cmp(&a.id));

    Ok(models)
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
/// All providers attempt to fetch models dynamically from their respective APIs.
/// Falls back to hardcoded lists if the API call fails.
///
/// - For `"anthropic"`: fetches from `GET /v1/models`, falls back to hardcoded list.
/// - For `"gemini"`: fetches from `GET /v1beta/models`, falls back to hardcoded list.
/// - For everything else: fetches from the OpenAI-compatible `GET /models` endpoint.
pub async fn list_models(
    provider: &str,
    api_key: &str,
    base_url: Option<&str>,
) -> Result<Vec<ModelInfo>, LlmError> {
    match provider {
        "anthropic" => match fetch_anthropic_models(api_key).await {
            Ok(models) if !models.is_empty() => Ok(models),
            Ok(_) => {
                debug!("Anthropic API returned empty model list, using fallback");
                Ok(anthropic_known_models())
            }
            Err(e) => {
                debug!("Failed to fetch Anthropic models, using fallback: {}", e);
                Ok(anthropic_known_models())
            }
        },
        "gemini" => match fetch_gemini_models(api_key).await {
            Ok(models) if !models.is_empty() => Ok(models),
            Ok(_) => {
                debug!("Gemini API returned empty model list, using fallback");
                Ok(gemini_known_models())
            }
            Err(e) => {
                debug!("Failed to fetch Gemini models, using fallback: {}", e);
                Ok(gemini_known_models())
            }
        },
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
    fn test_parse_anthropic_models_response() {
        let body = serde_json::json!({
            "data": [
                {
                    "id": "claude-opus-4-20250514",
                    "display_name": "Claude Opus 4",
                    "created_at": "2025-05-14T00:00:00Z",
                    "type": "model"
                },
                {
                    "id": "claude-sonnet-4-20250514",
                    "display_name": "Claude Sonnet 4",
                    "created_at": "2025-05-14T00:00:00Z",
                    "type": "model"
                },
                {
                    "id": "claude-3-5-haiku-20241022",
                    "display_name": "Claude 3.5 Haiku",
                    "created_at": "2024-10-22T00:00:00Z",
                    "type": "model"
                }
            ],
            "has_more": false,
            "first_id": "claude-opus-4-20250514",
            "last_id": "claude-3-5-haiku-20241022"
        });
        let models = parse_anthropic_models_response(&body).unwrap();
        assert_eq!(models.len(), 3);
        assert_eq!(models[0].id, "claude-opus-4-20250514");
        assert_eq!(models[0].name, "Claude Opus 4");
        assert_eq!(models[0].context_window, Some(200_000));
        assert!(models.iter().all(|m| m.is_chat_model));
        assert!(models.iter().any(|m| m.id.contains("haiku")));
    }

    #[test]
    fn test_parse_anthropic_models_empty() {
        let body = serde_json::json!({"data": [], "has_more": false});
        let models = parse_anthropic_models_response(&body).unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn test_parse_anthropic_models_missing_field() {
        let body = serde_json::json!({"error": {"message": "invalid api key"}});
        let result = parse_anthropic_models_response(&body);
        assert!(result.is_err());
    }

    #[test]
    fn test_gemini_known_models_list() {
        let models = gemini_known_models();
        assert!(models.len() >= 4);
        assert!(models.iter().all(|m| m.is_chat_model));
        assert!(models.iter().all(|m| m.context_window.is_some()));
        assert!(models.iter().any(|m| m.id.contains("flash")));
        assert!(models.iter().any(|m| m.id.contains("pro")));
        assert!(models.iter().any(|m| m.id.contains("2.5")));
    }

    #[test]
    fn test_parse_gemini_models_response() {
        let body = serde_json::json!({
            "models": [
                {
                    "name": "models/gemini-2.5-pro",
                    "displayName": "Gemini 2.5 Pro",
                    "inputTokenLimit": 1048576,
                    "outputTokenLimit": 65536,
                    "supportedGenerationMethods": ["generateContent", "countTokens"]
                },
                {
                    "name": "models/gemini-2.5-flash",
                    "displayName": "Gemini 2.5 Flash",
                    "inputTokenLimit": 1048576,
                    "outputTokenLimit": 65536,
                    "supportedGenerationMethods": ["generateContent", "countTokens"]
                },
                {
                    "name": "models/text-embedding-004",
                    "displayName": "Text Embedding 004",
                    "inputTokenLimit": 2048,
                    "supportedGenerationMethods": ["embedContent"]
                },
                {
                    "name": "models/aqa",
                    "displayName": "Model for AQA",
                    "inputTokenLimit": 7168,
                    "supportedGenerationMethods": ["generateAnswer"]
                }
            ]
        });
        let models = parse_gemini_models_response(&body).unwrap();
        // Only generateContent models, excluding embedding and aqa
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| m.id == "gemini-2.5-pro"));
        assert!(models.iter().any(|m| m.id == "gemini-2.5-flash"));
        assert_eq!(models[0].context_window, Some(1_048_576));
        // text-embedding and aqa should be excluded
        assert!(!models.iter().any(|m| m.id.contains("embedding")));
        assert!(!models.iter().any(|m| m.id.contains("aqa")));
    }

    #[test]
    fn test_parse_gemini_models_empty() {
        let body = serde_json::json!({"models": []});
        let models = parse_gemini_models_response(&body).unwrap();
        assert!(models.is_empty());
    }

    #[test]
    fn test_parse_gemini_models_missing_field() {
        let body = serde_json::json!({"error": "bad"});
        let result = parse_gemini_models_response(&body);
        assert!(result.is_err());
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

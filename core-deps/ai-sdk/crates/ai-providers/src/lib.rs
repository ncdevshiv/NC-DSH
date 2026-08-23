//! Provider adapters: real HTTP integrations with LLM APIs.
//!
//! - [`openai_compat`] — full OpenAI Chat Completions wire protocol adapter
//!   (`openai`, `openrouter`, `ollama`, arbitrary OpenAI-compatible
//!   gateways). Streaming, tool calling, structured output, vision,
//!   DeepSeek-style reasoning, cache-aware usage.
//! - [`anthropic`] — native Anthropic Messages API adapter (`x-api-key` +
//!   `anthropic-version`, tool use blocks, SSE streaming).
//! - [`gemini`] — native Google Gemini generateContent adapter.
//! - [`finetune`] — real OpenAI fine-tuning jobs API client.
//!
//! Nothing here is mocked: every adapter performs real HTTP requests.
//! Anthropic/Gemini/fine-tuning require their own credentials; without
//! them the adapters compile and are wire-tested, and integration tests
//! are credential-gated (documented per ENGINEERING-SPEC §40).

pub mod anthropic;
pub mod assistants;
pub mod batch;
pub mod files;
pub mod finetune;
pub mod gemini;
pub mod http;
pub mod images;
pub mod moderation;
pub mod openai_compat;
pub mod vector_stores;

use std::sync::Arc;

use ai_core::Provider;
use ai_errors::{AiError, ConfigurationError};
use anthropic::{AnthropicConfig, AnthropicProvider};
use gemini::{GeminiConfig, GeminiProvider};
use openai_compat::{OpenAiCompatConfig, OpenAiCompatProvider};

/// Creates a provider adapter from its id and configuration.
///
/// Native wire formats: `anthropic`, `google` (Gemini).
/// OpenAI-compatible: `openai`, `openrouter`, `ollama`, and any custom id
/// with an explicit `base_url` (e.g. the project gateway `opencode`).
pub fn create_provider(
    id: &str,
    config: &ai_config::ProviderConfig,
) -> Result<Arc<dyn Provider>, AiError> {
    match id {
        "anthropic" => Ok(Arc::new(AnthropicProvider::new(
            AnthropicConfig::from_provider_config(config)?,
        )?)),
        "google" => Ok(Arc::new(GeminiProvider::new(
            GeminiConfig::from_provider_config(config)?,
        )?)),
        _ => {
            let openai_compat_config = OpenAiCompatConfig::from_provider_config(id, config)?;
            Ok(Arc::new(OpenAiCompatProvider::new(openai_compat_config)?))
        }
    }
}

/// Builds a provider from a raw id, api key, and base URL (programmatic
/// configuration path). Native ids route to their native adapters.
pub fn create_provider_direct(
    id: impl Into<String>,
    api_key: impl Into<String>,
    base_url: impl Into<String>,
) -> Result<Arc<dyn Provider>, AiError> {
    let id = id.into();
    match id.as_str() {
        "anthropic" => Ok(Arc::new(AnthropicProvider::new(AnthropicConfig::new(
            api_key.into(),
        ))?)),
        "google" => Ok(Arc::new(GeminiProvider::new(GeminiConfig::new(
            api_key.into(),
        ))?)),
        _ => {
            let config = OpenAiCompatConfig::new(id, api_key, base_url);
            Ok(Arc::new(OpenAiCompatProvider::new(config)?))
        }
    }
}

/// Builds the AI SDK client from a [`ai_config::Config`], registering every
/// configured provider.
pub fn client_from_config(config: &ai_config::Config) -> Result<ai_core::AiClient, AiError> {
    config.validate()?;
    let mut builder = ai_core::AiClient::builder();
    for (name, provider_config) in &config.providers {
        if provider_config.api_key.is_some() {
            builder = builder.provider(create_provider(name, provider_config)?);
        }
    }
    builder.build().map_err(|e| {
        AiError::Configuration(ConfigurationError::with_source(
            "providers",
            "failed to build AiClient from config",
            e,
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_without_key_fails_with_typed_error() {
        let cfg = ai_config::ProviderConfig::default();
        let err = match create_provider("openai", &cfg) {
            Ok(_) => panic!("expected an error for a provider without an API key"),
            Err(e) => e,
        };
        assert!(matches!(err, AiError::Configuration(_)));
        assert!(err.to_string().contains("OPENAI_API_KEY"), "{err}");
    }

    #[test]
    fn native_providers_route_to_native_adapters() {
        let anthropic_cfg = ai_config::ProviderConfig::new("sk-anthropic-test");
        let provider = create_provider("anthropic", &anthropic_cfg).unwrap();
        assert_eq!(provider.id(), "anthropic");

        let google_cfg = ai_config::ProviderConfig::new("sk-google-test");
        let provider = create_provider("google", &google_cfg).unwrap();
        assert_eq!(provider.id(), "google");
    }

    #[test]
    fn openai_compat_provider_debug_redacts_key() {
        let provider = OpenAiCompatProvider::new(OpenAiCompatConfig::new(
            "opencode",
            "sk-super-secret",
            "https://opencode.ai/zen/go/v1",
        ))
        .unwrap();
        let debug = format!("{provider:?}");
        assert!(!debug.contains("sk-super-secret"), "{debug}");
        assert!(debug.contains("***redacted***"), "{debug}");
    }
}

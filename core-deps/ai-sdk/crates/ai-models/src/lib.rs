//! Model metadata, capabilities, and the model registry.
//!
//! Providers expose their models as [`ModelInfo`] with discoverable
//! [`ModelCapabilities`]; the registry keeps a curated catalog of known
//! models (context windows, token pricing) used for routing and cost
//! estimation.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use ai_errors::{AiError, ValidationError};
use ai_types::{Modality, ModelId, ProviderId};

/// Capabilities a model may support. Discovered programmatically from the
/// provider and/or the curated registry.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ModelCapabilities {
    /// Input modalities (e.g. text, image, audio).
    #[serde(default)]
    pub input_modalities: Vec<Modality>,
    /// Output modalities.
    #[serde(default)]
    pub output_modalities: Vec<Modality>,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_structured_output: bool,
    pub supports_embeddings: bool,
    pub supports_vision: bool,
    pub supports_fine_tuning: bool,
}

/// Pricing per 1,000 tokens (US dollars), used for cost estimation.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Pricing {
    /// USD per 1k input tokens.
    pub input_per_1k: f64,
    /// USD per 1k output tokens.
    pub output_per_1k: f64,
    /// USD per 1k cached-input tokens (when applicable).
    pub cached_input_per_1k: Option<f64>,
}

/// Full metadata for a model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ModelInfo {
    pub provider: ProviderId,
    pub id: ModelId,
    /// Display name (may equal id).
    pub name: String,
    /// Context window in tokens.
    pub context_window: u64,
    /// Maximum output tokens.
    pub max_output_tokens: u64,
    #[serde(default)]
    pub capabilities: ModelCapabilities,
    #[serde(default)]
    pub pricing: Option<Pricing>,
    /// Extra provider metadata (release date, aliases…).
    #[serde(default)]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

impl ModelInfo {
    pub fn new(
        provider: ProviderId,
        id: ModelId,
        context_window: u64,
        max_output_tokens: u64,
    ) -> Self {
        Self {
            provider,
            id,
            name: String::new(),
            context_window,
            max_output_tokens,
            capabilities: ModelCapabilities::default(),
            pricing: None,
            metadata: BTreeMap::new(),
        }
    }

    /// Builder-style setter for the display name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    pub fn with_capabilities(mut self, capabilities: ModelCapabilities) -> Self {
        self.capabilities = capabilities;
        self
    }

    pub fn with_pricing(mut self, pricing: Pricing) -> Self {
        self.pricing = Some(pricing);
        self
    }

    /// Estimates the cost of a token usage in USD using the model's pricing,
    /// or `None` when pricing is unknown.
    pub fn estimate_cost(
        &self,
        input_tokens: u64,
        output_tokens: u64,
        cached_input_tokens: u64,
    ) -> Option<f64> {
        let pricing = self.pricing?;
        let cached = cached_input_tokens as f64
            * pricing.cached_input_per_1k.unwrap_or(pricing.input_per_1k)
            / 1000.0;
        let input = (input_tokens.saturating_sub(cached_input_tokens)) as f64
            * pricing.input_per_1k
            / 1000.0;
        let output = output_tokens as f64 * pricing.output_per_1k / 1000.0;
        Some(cached + input + output)
    }
}

impl fmt::Display for ModelInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.provider, self.id)
    }
}

/// In-memory catalog of known models. Entries can be added at runtime;
/// providers can also enumerate their own models via their APIs.
#[derive(Debug, Clone, Default)]
pub struct ModelRegistry {
    models: BTreeMap<String, ModelInfo>,
}

impl ModelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a model, keyed by `provider:id`.
    pub fn register(&mut self, info: ModelInfo) {
        let key = format!("{}:{}", info.provider, info.id);
        self.models.insert(key, info);
    }

    /// Looks up a model by `provider:id` or `provider/model` style keys.
    pub fn get(&self, provider: &str, model: &str) -> Option<&ModelInfo> {
        self.models
            .get(&format!("{provider}:{model}"))
            .or_else(|| self.models.get(&format!("{provider}/{model}")))
    }

    pub fn get_by_key(&self, key: &str) -> Option<&ModelInfo> {
        self.models.get(key)
    }

    /// All registered models for a provider.
    pub fn for_provider(&self, provider: &str) -> Vec<&ModelInfo> {
        self.models
            .values()
            .filter(|m| m.provider.as_str() == provider)
            .collect()
    }

    pub fn len(&self) -> usize {
        self.models.len()
    }

    pub fn is_empty(&self) -> bool {
        self.models.is_empty()
    }

    /// Parses a `provider:model` (or `provider/model`) reference.
    pub fn parse_reference(reference: &str) -> Result<(String, String), AiError> {
        let (provider, model) = reference
            .split_once(':')
            .or_else(|| reference.split_once('/'))
            .ok_or_else(|| {
                AiError::Validation(ValidationError::new(format!(
                    "model reference `{reference}` must be `provider:model` or `provider/model`"
                )))
            })?;
        if provider.is_empty() || model.is_empty() {
            return Err(AiError::Validation(ValidationError::new(format!(
                "model reference `{reference}` has an empty provider or model part"
            ))));
        }
        Ok((provider.to_string(), model.to_string()))
    }
}

/// The curated default catalog of well-known models.
///
/// Values are from official provider pricing/documentation as of 2026-08.
/// Providers should prefer API-driven model lists when available; this
/// catalog exists for routing/cost estimation without extra API calls.
pub fn default_catalog() -> ModelRegistry {
    let mut registry = ModelRegistry::new();

    fn caps(
        input: &[Modality],
        output: &[Modality],
        streaming: bool,
        tools: bool,
        structured: bool,
        vision: bool,
    ) -> ModelCapabilities {
        ModelCapabilities {
            input_modalities: input.to_vec(),
            output_modalities: output.to_vec(),
            supports_streaming: streaming,
            supports_tools: tools,
            supports_structured_output: structured,
            supports_embeddings: false,
            supports_vision: vision,
            supports_fine_tuning: false,
        }
    }

    let text = &[Modality::Text];
    let text_image = &[Modality::Text, Modality::Image];

    // OpenAI
    // Pricing: published USD-per-1M values converted to per-1K (×0.001).
    type OpenAiRow = (
        &'static str,
        u64,
        u64,
        &'static [Modality],
        bool,
        f64,
        f64,
        Option<f64>,
    );
    let openai_models: [OpenAiRow; 5] = [
        (
            "gpt-4o",
            128_000,
            16_384,
            text_image,
            true,
            0.0025,
            0.01,
            Some(0.00125),
        ),
        (
            "gpt-4o-mini",
            128_000,
            16_384,
            text_image,
            true,
            0.00015,
            0.0006,
            Some(0.000075),
        ),
        (
            "gpt-4.1",
            1_047_576,
            32_768,
            text_image,
            true,
            0.002,
            0.008,
            Some(0.0005),
        ),
        (
            "gpt-4.1-mini",
            1_047_576,
            32_768,
            text_image,
            true,
            0.0004,
            0.0016,
            Some(0.0001),
        ),
        (
            "o3-mini", 200_000, 100_000, text, false, 0.0011, 0.0044, None,
        ),
    ];
    for (id, ctx, max_out, input, vision, price_in, price_out, cache_in) in openai_models {
        registry.register(
            ModelInfo::new(ProviderId::new("openai"), ModelId::new(id), ctx, max_out)
                .with_name(id)
                .with_capabilities(caps(input, text, true, true, true, vision))
                .with_pricing(Pricing {
                    input_per_1k: price_in,
                    output_per_1k: price_out,
                    cached_input_per_1k: cache_in,
                }),
        );
    }

    // Anthropic
    for (id, ctx, max_out, price_in, price_out, cache_in) in [
        (
            "claude-sonnet-4-20250514",
            200_000_u64,
            64_000_u64,
            0.003,
            0.015,
            Some(0.0003),
        ),
        (
            "claude-3-5-sonnet-20241022",
            200_000,
            64_000,
            0.003,
            0.015,
            Some(0.0003),
        ),
        (
            "claude-3-opus-20240229",
            200_000,
            32_000,
            0.015,
            0.075,
            Some(0.0015),
        ),
        (
            "claude-3-haiku-20240307",
            200_000,
            4_096,
            0.00025,
            0.00125,
            Some(0.00003),
        ),
    ] {
        registry.register(
            ModelInfo::new(ProviderId::new("anthropic"), ModelId::new(id), ctx, max_out)
                .with_name(id)
                .with_capabilities(caps(text_image, text, true, true, true, true))
                .with_pricing(Pricing {
                    input_per_1k: price_in,
                    output_per_1k: price_out,
                    cached_input_per_1k: cache_in,
                }),
        );
    }

    // Google Gemini
    for (id, ctx, max_out, price_in, price_out) in [
        ("gemini-1.5-pro", 2_000_000_u64, 8_192_u64, 0.00125, 0.005),
        ("gemini-1.5-flash", 1_000_000, 8_192, 0.000075, 0.0003),
        ("gemini-2.0-flash", 1_000_000, 8_192, 0.0001, 0.0004),
    ] {
        registry.register(
            ModelInfo::new(ProviderId::new("google"), ModelId::new(id), ctx, max_out)
                .with_name(id)
                .with_capabilities(caps(text_image, text, true, true, true, true))
                .with_pricing(Pricing {
                    input_per_1k: price_in,
                    output_per_1k: price_out,
                    cached_input_per_1k: None,
                }),
        );
    }

    // OpenRouter (proxy; per-model pricing varies — generic fallback).
    registry.register(
        ModelInfo::new(
            ProviderId::new("openrouter"),
            ModelId::new("auto"),
            200_000,
            32_000,
        )
        .with_name("auto")
        .with_capabilities(caps(text_image, text, true, true, true, true)),
    );

    // Ollama (local; free).
    registry.register(
        ModelInfo::new(
            ProviderId::new("ollama"),
            ModelId::new("llama3.1"),
            128_000,
            8_192,
        )
        .with_name("llama3.1")
        .with_capabilities(caps(text, text, true, true, true, false))
        .with_pricing(Pricing {
            input_per_1k: 0.0,
            output_per_1k: 0.0,
            cached_input_per_1k: Some(0.0),
        }),
    );

    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_has_known_models() {
        let catalog = default_catalog();
        assert!(catalog.len() >= 10, "catalog too small: {}", catalog.len());
        let gpt4o = catalog.get("openai", "gpt-4o").expect("gpt-4o registered");
        assert!(gpt4o.capabilities.supports_tools);
        assert!(gpt4o.capabilities.supports_vision);
        assert_eq!(gpt4o.pricing.unwrap().input_per_1k, 0.0025);
    }

    #[test]
    fn cost_estimation_uses_pricing() {
        let catalog = default_catalog();
        let gpt4o = catalog.get("openai", "gpt-4o").unwrap();
        let cost = gpt4o.estimate_cost(1_000, 1_000, 0).expect("pricing known");
        assert!((cost - 0.0125).abs() < 1e-9, "expected $0.0125, got {cost}");
        // Cached input tokens are discounted.
        let cached = gpt4o.estimate_cost(1_000, 1_000, 1_000).unwrap();
        assert!(cached < cost, "cached tokens should cost less");
    }

    #[test]
    fn parse_references() {
        assert_eq!(
            ModelRegistry::parse_reference("openai:gpt-4o").unwrap(),
            ("openai".into(), "gpt-4o".into())
        );
        assert_eq!(
            ModelRegistry::parse_reference("openai/gpt-4o").unwrap(),
            ("openai".into(), "gpt-4o".into())
        );
        assert!(ModelRegistry::parse_reference("gpt-4o").is_err());
        assert!(ModelRegistry::parse_reference(":gpt-4o").is_err());
    }

    #[test]
    fn registry_provider_filter() {
        let catalog = default_catalog();
        let openai_models = catalog.for_provider("openai");
        assert!(!openai_models.is_empty());
        assert!(
            openai_models
                .iter()
                .all(|m| m.provider.as_str() == "openai")
        );
    }
}

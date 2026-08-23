//! Core traits and primitives for the AI SDK.
//!
//! This crate defines the abstractions every subsystem builds on:
//!
//! - [`Model`] — generate / stream completions for one model
//! - [`Provider`] — a vendor adapter that exposes models
//! - [`AiClient`] — the ergonomic entry point (builder + `generate`/`stream`)
//! - Request/response shapes ([`ChatRequest`], [`ToolDefinition`])
//!
//! See [`ai-providers`] for real implementations and [`ai-runtime`] for
//! parallel execution and resilience on top of these traits.

use std::collections::HashMap;
use std::fmt;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use ai_errors::{AiError, ValidationError};
use ai_models::ModelRegistry;

// Convenience re-exports: any [`Model`] implementor outside this crate needs
// these names, and importing them through `ai-core` keeps the public surface
// of dependent crates free of transitive dependencies. The internal code
// below references these same names through these public imports.
pub use ai_models::ModelInfo;
pub use ai_types::{Completion, Message, Role, StreamEvent};

/// Description of a tool the model may call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    /// JSON Schema for the tool's input arguments.
    pub input_schema: serde_json::Value,
}

impl ToolDefinition {
    pub fn new(
        name: impl Into<String>,
        description: impl Into<String>,
        input_schema: serde_json::Value,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            input_schema,
        }
    }

    /// Validates that the definition is well-formed enough to send to a
    /// provider: non-empty name and an object-typed schema.
    pub fn validate(&self) -> Result<(), AiError> {
        if self.name.is_empty() {
            return Err(AiError::Validation(ValidationError::new(
                "tool name must not be empty",
            )));
        }
        if !self.input_schema.is_object() {
            return Err(AiError::Validation(ValidationError::new(format!(
                "tool `{}` input schema must be a JSON Schema object",
                self.name
            ))));
        }
        Ok(())
    }
}

/// Requested output format for structured output support.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResponseFormat {
    /// Free-form text (default).
    #[default]
    Text,
    /// Guaranteed valid JSON object.
    JsonObject,
    /// JSON matching the provided schema (provider-specific enforcement).
    JsonSchema {
        schema: serde_json::Value,
        name: String,
    },
}

/// Reasoning effort requested for reasoning models (e.g. OpenAI o1, o3-mini).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
}

impl fmt::Display for ReasoningEffort {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        };
        f.write_str(s)
    }
}

/// A request to a language model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChatRequest {
    /// Conversation history; at least one message is required.
    pub messages: Vec<Message>,
    /// Tool definitions made available to the model.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolDefinition>,
    /// Temperature in `[0, 2]`. Lower values are more deterministic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Nucleus sampling: cumulative probability cutoff in `(0, 1]`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Frequency penalty in `[-2, 2]` (OpenAI-compatible providers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f32>,
    /// Presence penalty in `[-2, 2]` (OpenAI-compatible providers).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f32>,
    /// Maximum number of output tokens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u64>,
    /// Structured output request.
    #[serde(default)]
    pub response_format: ResponseFormat,
    /// Reasoning effort level for reasoning models (e.g. `low`, `medium`, `high`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_effort: Option<ReasoningEffort>,
    /// Seed for deterministic sampling.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
    /// End-user identifier for safety/abuse monitoring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    /// Whether to enable parallel tool execution on supported models.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parallel_tool_calls: Option<bool>,
    /// Sequences that stop generation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop: Vec<String>,
    /// Provider-specific options passed through verbatim (e.g.
    /// `{"cache_control": {"type": "ephemeral"}}` for Anthropic). Keys
    /// nested under `extra_body` are merged into the request body.
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub provider_options: serde_json::Value,
}

impl Default for ChatRequest {
    fn default() -> Self {
        Self {
            messages: Vec::new(),
            tools: Vec::new(),
            temperature: None,
            top_p: None,
            frequency_penalty: None,
            presence_penalty: None,
            max_tokens: None,
            response_format: ResponseFormat::Text,
            reasoning_effort: None,
            seed: None,
            user: None,
            parallel_tool_calls: None,
            stop: Vec::new(),
            provider_options: serde_json::Value::Null,
        }
    }
}

impl ChatRequest {
    pub fn new(messages: Vec<Message>) -> Self {
        Self {
            messages,
            ..Default::default()
        }
    }

    pub fn with_tools(mut self, tools: Vec<ToolDefinition>) -> Self {
        self.tools = tools;
        self
    }

    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = Some(top_p);
        self
    }

    pub fn with_frequency_penalty(mut self, penalty: f32) -> Self {
        self.frequency_penalty = Some(penalty);
        self
    }

    pub fn with_presence_penalty(mut self, penalty: f32) -> Self {
        self.presence_penalty = Some(penalty);
        self
    }

    pub fn with_max_tokens(mut self, max_tokens: u64) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    pub fn with_response_format(mut self, format: ResponseFormat) -> Self {
        self.response_format = format;
        self
    }

    pub fn with_reasoning_effort(mut self, effort: ReasoningEffort) -> Self {
        self.reasoning_effort = Some(effort);
        self
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    pub fn with_user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    pub fn with_parallel_tool_calls(mut self, enable: bool) -> Self {
        self.parallel_tool_calls = Some(enable);
        self
    }

    pub fn with_stop(mut self, stop: Vec<String>) -> Self {
        self.stop = stop;
        self
    }

    /// Validates the request: non-empty messages, valid tools, sane
    /// temperature range.
    pub fn validate(&self) -> Result<(), AiError> {
        if self.messages.is_empty() {
            return Err(AiError::Validation(ValidationError::new(
                "request must contain at least one message",
            )));
        }
        for tool in &self.tools {
            tool.validate()?;
        }
        if let Some(t) = self.temperature {
            if !(0.0..=2.0).contains(&t) {
                return Err(AiError::Validation(ValidationError::new(format!(
                    "temperature {t} is outside the valid range [0, 2]"
                ))));
            }
        }
        Ok(())
    }
}

/// A boxed, unified stream of [`StreamEvent`]s.
pub type EventStream =
    Pin<Box<dyn futures_core::Stream<Item = Result<StreamEvent, AiError>> + Send>>;

/// A single model behind a provider.
///
/// Implementations are responsible for serializing the request into the
/// provider's wire format, authenticating, and mapping the response back to
/// [`Completion`] / [`StreamEvent`]s. Implementations must be cancellation
/// safe (drop the future → abort the request).
#[async_trait]
pub trait Model: Send + Sync {
    /// Metadata for this model instance.
    fn info(&self) -> &ModelInfo;

    /// Produces a complete (non-streaming) completion.
    async fn generate(&self, request: ChatRequest) -> Result<Completion, AiError>;

    /// Produces a unified stream of events. When the provider supports
    /// streaming natively this is a true streaming call; otherwise the
    /// implementation may buffer and replay. The returned stream must be
    /// cancellable by dropping it.
    async fn stream(&self, request: ChatRequest) -> Result<EventStream, AiError>;
}

/// A provider adapter exposing one or more models.
///
/// Implementations hold their own HTTP client, base URL, and credentials.
#[async_trait]
pub trait Provider: Send + Sync {
    /// Provider identifier (`openai`, `anthropic`, …).
    fn id(&self) -> &str;

    /// Enumerates models via the provider's API when available.
    async fn list_models(&self) -> Result<Vec<ModelInfo>, AiError>;

    /// Resolves a model id to a usable [`Model`] handle.
    fn model(&self, model_id: &str) -> Result<Arc<dyn Model>, AiError>;
}

/// The unified entry point for the SDK.
///
/// ```no_run
/// # async fn example() -> ai_core::Result<()> {
/// use ai_core::AiClient;
/// use ai_types::{Message, Role};
///
/// let client = AiClient::builder().build()?;
/// let completion = client
///     .generate("openai:gpt-4o", vec![Message::text(Role::User, "Hello")])
///     .await?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct AiClient {
    providers: HashMap<String, Arc<dyn Provider>>,
    registry: ModelRegistry,
    /// Default provider used when a model reference has no provider prefix.
    default_provider: Option<String>,
    /// Pre-wrapped models registered directly by reference (the decoration
    /// seam — see [`AiClient::register_model`]). Shared via `Arc<RwLock<_>>`
    /// so resilience can be installed after construction without making the
    /// client generically mutable.
    models: Arc<RwLock<HashMap<String, Arc<dyn Model>>>>,
}

/// Builder for [`AiClient`].
#[derive(Default)]
pub struct AiClientBuilder {
    providers: HashMap<String, Arc<dyn Provider>>,
    registry: Option<ModelRegistry>,
    default_provider: Option<String>,
    models: HashMap<String, Arc<dyn Model>>,
}

impl AiClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a provider adapter under its own id.
    pub fn provider(mut self, provider: Arc<dyn Provider>) -> Self {
        self.providers.insert(provider.id().to_string(), provider);
        self
    }

    /// Sets the model registry used for routing metadata (defaults to the
    /// curated catalog).
    pub fn registry(mut self, registry: ModelRegistry) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Sets the provider used when a model reference omits the provider
    /// prefix (e.g. `gpt-4o` instead of `openai:gpt-4o`).
    pub fn default_provider(mut self, provider: &str) -> Self {
        self.default_provider = Some(provider.to_string());
        self
    }

    /// Registers a pre-built [`Model`] under an exact model reference.
    ///
    /// This is the dependency-inversion seam for higher-level crates: the
    /// client resolves `provider:model` references through registered
    /// providers, but it cannot depend on resilience/parallelism layers
    /// (`ai-runtime`) without creating a cycle — `ai-runtime` already
    /// depends on `ai-core`. Instead of inlining decoration logic here,
    /// callers wrap models however they like and hand the finished,
    /// possibly decorated `Arc<dyn Model>` to the client:
    ///
    /// ```ignore
    /// // in a crate that sees both ai-core and ai-runtime:
    /// let bare = client.resolve_model("openai:gpt-4o")?.1;
    /// let resilient = Arc::new(ResilientModel::new(bare, policy));
    /// client.register_model("openai:gpt-4o", resilient);
    /// ```
    ///
    /// A registered model takes precedence over provider resolution for its
    /// exact reference string. The default (no registrations) preserves the
    /// historical behavior exactly.
    pub fn register_model(mut self, reference: impl Into<String>, model: Arc<dyn Model>) -> Self {
        self.models.insert(reference.into(), model);
        self
    }

    pub fn build(self) -> Result<AiClient, AiError> {
        Ok(AiClient {
            providers: self.providers,
            registry: self.registry.unwrap_or_else(ai_models::default_catalog),
            default_provider: self.default_provider,
            models: Arc::new(RwLock::new(self.models)),
        })
    }
}

impl AiClient {
    pub fn builder() -> AiClientBuilder {
        AiClientBuilder::new()
    }

    /// Looks up a registered provider.
    pub fn provider(&self, name: &str) -> Result<Arc<dyn Provider>, AiError> {
        self.providers.get(name).cloned().ok_or_else(|| {
            AiError::Validation(ValidationError::new(format!(
                "provider `{name}` is not registered with this client; \
                 register it via AiClient::builder().provider(..)"
            )))
        })
    }

    pub fn registry(&self) -> &ModelRegistry {
        &self.registry
    }

    /// Registers a pre-built (possibly resilience-decorated) [`Model`] on an
    /// already-constructed client. See [`AiClientBuilder::register_model`]
    /// for the design rationale.
    ///
    /// Registration is interior-mutable so resilience layers can decorate
    /// models after the builder has produced the client; concurrent readers
    /// observe either the previous or the new model for a reference, never a
    /// torn state.
    pub fn register_model(&self, reference: impl Into<String>, model: Arc<dyn Model>) {
        self.models
            .write()
            .expect("model registry lock not poisoned")
            .insert(reference.into(), model);
    }

    /// Model references that currently resolve through the registration
    /// seam rather than provider resolution (sorted).
    pub fn registered_references(&self) -> Vec<String> {
        let mut refs: Vec<String> = self
            .models
            .read()
            .expect("model registry lock not poisoned")
            .keys()
            .cloned()
            .collect();
        refs.sort();
        refs
    }

    /// Resolves a `provider:model` (or `provider/model`) reference. A bare
    /// model id resolves against the configured default provider.
    ///
    /// Resolution order: models registered via [`AiClient::register_model`]
    /// (exact reference match) first — they are pre-wrapped decorations —
    /// then registered providers.
    pub fn resolve_model(&self, reference: &str) -> Result<(String, Arc<dyn Model>), AiError> {
        if let Some(model) = self
            .models
            .read()
            .expect("model registry lock not poisoned")
            .get(reference)
            .cloned()
        {
            let provider_name = ModelRegistry::parse_reference(reference)
                .map(|(p, _)| p)
                .unwrap_or_else(|_| reference.to_string());
            return Ok((provider_name, model));
        }
        let (provider_name, model_id) = match ModelRegistry::parse_reference(reference) {
            Ok((p, m)) => (p, m),
            Err(_) => {
                let default = self.default_provider.as_deref().ok_or_else(|| {
                    AiError::Validation(ValidationError::new(format!(
                        "model reference `{reference}` has no provider prefix and no \
                         default provider is configured"
                    )))
                })?;
                (default.to_string(), reference.to_string())
            }
        };
        let provider = self.provider(&provider_name)?;
        let model = provider.model(&model_id)?;
        Ok((provider_name, model))
    }

    /// Generates a completion. Errors from the model are returned as-is.
    pub async fn generate(
        &self,
        reference: &str,
        messages: Vec<Message>,
    ) -> Result<Completion, AiError> {
        let (_provider, model) = self.resolve_model(reference)?;
        let request = ChatRequest::new(messages);
        request.validate()?;
        model.generate(request).await
    }

    /// Generates a completion from a fully-specified request.
    pub async fn generate_request(
        &self,
        reference: &str,
        request: ChatRequest,
    ) -> Result<Completion, AiError> {
        let (_provider, model) = self.resolve_model(reference)?;
        request.validate()?;
        model.generate(request).await
    }

    /// Streams a completion.
    pub async fn stream(
        &self,
        reference: &str,
        messages: Vec<Message>,
    ) -> Result<EventStream, AiError> {
        let (_provider, model) = self.resolve_model(reference)?;
        let request = ChatRequest::new(messages);
        request.validate()?;
        model.stream(request).await
    }

    /// Streams a completion from a fully-specified request.
    pub async fn stream_request(
        &self,
        reference: &str,
        request: ChatRequest,
    ) -> Result<EventStream, AiError> {
        let (_provider, model) = self.resolve_model(reference)?;
        request.validate()?;
        model.stream(request).await
    }

    /// Convenience: resolves a model id from the curated registry, useful
    /// for routing/cost logic without hitting provider APIs.
    pub fn model_info(&self, provider: &str, model: &str) -> Option<&ModelInfo> {
        self.registry.get(provider, model)
    }

    pub fn provider_ids(&self) -> Vec<String> {
        let mut ids: Vec<String> = self.providers.keys().cloned().collect();
        ids.sort();
        ids
    }
}

/// Convenience alias for `ai-errors::Result`.
pub type Result<T, E = AiError> = ai_errors::Result<T, E>;

/// Re-exports for ergonomic single-import usage.
pub mod prelude {
    pub use crate::{
        AiClient, ChatRequest, EventStream, Model, Provider, ResponseFormat, ToolDefinition,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai_types::{ContentPart, Role};

    #[test]
    fn tool_definition_validation() {
        let ok = ToolDefinition::new("calc", "calculates", serde_json::json!({"type": "object"}));
        assert!(ok.validate().is_ok());

        let bad_schema = ToolDefinition::new("calc", "desc", serde_json::json!([1, 2]));
        assert!(bad_schema.validate().is_err());

        let empty_name = ToolDefinition::new("", "desc", serde_json::json!({"type": "object"}));
        assert!(empty_name.validate().is_err());
    }

    #[test]
    fn request_validation() {
        assert!(ChatRequest::default().validate().is_err());
        let req = ChatRequest::new(vec![Message::text(Role::User, "hi")]);
        assert!(req.validate().is_ok());
        let bad_temp = req.clone().with_temperature(3.0);
        assert!(bad_temp.validate().is_err());
    }

    #[test]
    fn response_format_tags() {
        let f = ResponseFormat::JsonSchema {
            schema: serde_json::json!({"type": "object"}),
            name: "result".into(),
        };
        let json = serde_json::to_string(&f).unwrap();
        assert!(json.contains("\"type\":\"json_schema\""), "{json}");
    }

    #[test]
    fn unresolved_provider_is_typed_error() {
        let client = AiClient::builder().build().unwrap();
        let err = match client.provider("openai") {
            Ok(_) => panic!("expected an error for an unregistered provider"),
            Err(e) => e,
        };
        assert!(matches!(err, AiError::Validation(_)));
        assert!(err.to_string().contains("openai"));
    }

    #[test]
    fn resolve_model_requires_provider_prefix_without_default() {
        let client = AiClient::builder().build().unwrap();
        let err = match client.resolve_model("gpt-4o") {
            Ok(_) => panic!("expected an error for a bare model id without default provider"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("no provider prefix"));
    }

    #[test]
    fn default_provider_used_for_bare_ids() {
        // A provider registered with a mock model is exercised in
        // ai-providers' integration tests; here we only assert the
        // resolution path reports the right error for an unknown model.
        let client = AiClient::builder()
            .default_provider("openai")
            .build()
            .unwrap();
        let err = match client.resolve_model("gpt-4o") {
            Ok(_) => panic!("expected an error: provider not registered"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("openai"), "{err}");
    }

    // Ensure prelude compiles with the expected names.
    #[allow(dead_code)]
    fn prelude_names() {
        let _: fn() -> AiClientBuilder = AiClient::builder;
        let _: Option<ContentPart> = None;
        let _: Option<Role> = None;
    }

    // ---- register_model seam -------------------------------------------------

    use std::sync::atomic::{AtomicU32, Ordering};

    struct CountingModel {
        calls: AtomicU32,
        text: &'static str,
    }

    impl CountingModel {
        fn new(text: &'static str) -> Arc<Self> {
            Arc::new(Self {
                calls: AtomicU32::new(0),
                text,
            })
        }
    }

    #[async_trait]
    impl Model for CountingModel {
        fn info(&self) -> &ModelInfo {
            static INFO: std::sync::OnceLock<ModelInfo> = std::sync::OnceLock::new();
            INFO.get_or_init(|| {
                ModelInfo::new(
                    ai_types::ProviderId::new("mock"),
                    ai_types::ModelId::new("counting"),
                    1_000,
                    1_000,
                )
            })
        }

        async fn generate(&self, _request: ChatRequest) -> Result<Completion, AiError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(Completion {
                provider: self.info().provider.clone(),
                model: self.info().id.clone(),
                text: self.text.to_string(),
                tool_calls: Vec::new(),
                usage: Default::default(),
                reasoning: None,
                raw: serde_json::Value::Null,
                finish_reason: Some("stop".into()),
            })
        }

        async fn stream(&self, _request: ChatRequest) -> Result<EventStream, AiError> {
            Err(AiError::Internal(ai_errors::InternalError::new(
                "not implemented",
            )))
        }
    }

    #[tokio::test]
    async fn registered_model_routes_generate_through_the_seam() {
        let model = CountingModel::new("via-seam");
        let client = AiClient::builder()
            .register_model("mock:counting", Arc::clone(&model) as Arc<dyn Model>)
            .build()
            .unwrap();

        let completion = client
            .generate("mock:counting", vec![Message::text(Role::User, "hi")])
            .await
            .unwrap();
        assert_eq!(completion.text, "via-seam");
        assert_eq!(model.calls.load(Ordering::SeqCst), 1);

        // The registration is visible through introspection and resolution.
        assert_eq!(client.registered_references(), vec!["mock:counting"]);
        let (provider, resolved) = client.resolve_model("mock:counting").unwrap();
        assert_eq!(provider, "mock");
        assert!(Arc::ptr_eq(
            &resolved,
            &(Arc::clone(&model) as Arc<dyn Model>)
        ));
    }

    #[tokio::test]
    async fn registered_model_takes_precedence_over_provider_resolution() {
        let provider_model = CountingModel::new("from-provider");
        let decorated = CountingModel::new("decorated");
        let provider_model: Arc<CountingModel> = provider_model;

        struct StaticProvider(Arc<CountingModel>);
        #[async_trait]
        impl Provider for StaticProvider {
            fn id(&self) -> &str {
                "mock"
            }
            async fn list_models(&self) -> Result<Vec<ModelInfo>, AiError> {
                Ok(vec![self.0.info().clone()])
            }
            fn model(&self, _model_id: &str) -> Result<Arc<dyn Model>, AiError> {
                Ok(Arc::clone(&self.0) as Arc<dyn Model>)
            }
        }

        let client = AiClient::builder()
            .provider(Arc::new(StaticProvider(Arc::clone(&provider_model))))
            .register_model("mock:counting", Arc::clone(&decorated) as Arc<dyn Model>)
            .build()
            .unwrap();

        let completion = client
            .generate("mock:counting", vec![Message::text(Role::User, "hi")])
            .await
            .unwrap();
        assert_eq!(completion.text, "decorated");
        assert_eq!(
            decorated.calls.load(Ordering::SeqCst),
            1,
            "the registered (decorated) model must serve"
        );
        assert_eq!(
            provider_model.calls.load(Ordering::SeqCst),
            0,
            "provider resolution must be shadowed by the registration"
        );

        // Other references of the same provider still resolve via providers.
        let completion = client
            .generate("mock:other", vec![Message::text(Role::User, "hi")])
            .await
            .unwrap();
        assert_eq!(completion.text, "from-provider");
    }

    #[test]
    fn default_client_has_no_registrations_backward_compat() {
        let client = AiClient::builder().build().unwrap();
        assert!(client.registered_references().is_empty());
    }
}
